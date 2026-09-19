#!/usr/bin/env python3
"""Track support projectiles and expose the exact replay-only simulation boundary.

This is research tooling, not parser output.  It decodes the four support
projectile creation families, binds each creation to the shipped support/effect
catalogue, gives reused wire IDs stable ``(projectileId, creationOffset)``
lifecycle keys, and inventories later effect records.  It never joins an effect
or a death to a projectile unless the replay contains a deterministic bridge.

The b35 server updates projectile movers with a per-frame float delta and then
performs collision/world queries.  Replays serialize the creation state but not
that delta stream, collision result, or a projectile ID on effect records.  The
free-flight functions below therefore accept an explicit delta sequence for
controlled tests; corpus rows abstain with ``unsupported`` when it is absent.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import multiprocessing
import os
import pathlib
import struct
from collections import Counter
from dataclasses import asdict, dataclass

from wic_bintag import (
    TYPE_FLOAT32,
    TYPE_INT32,
    TYPE_UINT32,
    decompress,
    fields,
    name_hash,
    walk,
)
from wic_ice import SupportProjectileDefinition, shipped_support_projectile_definitions

SCHEMA_VERSION = 1
RECORDING_RESET_DROP_SECONDS = 1.0
GRAVITY = 9.81

SERVER_EXE_SHA256 = "c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf"
CLIENT_EXE_SHA256 = "41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc"
SERVER_SDF_SHA256 = "fd6bbe78096f1f5af77cf66cb08423107b54f8cafb25112ebce7a650bd7086b3"

SUPPORT_FAMILIES = {
    name_hash("ProjectileStraightSupportCreate"): "straight",
    name_hash("ProjectileBallisticSupportCreate"): "ballistic",
    name_hash("ProjectileHomingSupportCreate_Position"): "homingPosition",
    name_hash("ProjectileHomingSupportCreate_Unit"): "homingUnit",
}
EXPECTED_MOVER = {
    "straight": "STRAIGHT",
    "ballistic": "BALLISTIC",
    "homingPosition": "HOMING",
    "homingUnit": "HOMING",
}
NORMAL_FAMILIES = {
    name_hash("ProjectileStraightCreate"): "straight",
    name_hash("ProjectileBallisticCreate"): "ballistic",
    name_hash("ProjectileHomingTargetCreate"): "homingPosition",
    name_hash("ProjectileHomingUnitCreate"): "homingUnit",
}

MSG_CREATE_CLOUD = name_hash("CreateCloud")
MSG_SPAWN_EXPLOSION = name_hash("SpawnExplosion")
MSG_SPAWN_EXPLOSION_WITH_CRATER = name_hash("SpawnExplosionWithCrater")
MSG_UNIT_CREATE = name_hash("UnitCreate")

FIELD_AN_ID = name_hash("anId")
FIELD_A_PROJECTILE = name_hash("aProjectile")
FIELD_A_SUPPORT_THING = name_hash("aSupportThing")
FIELD_A_POSITION = tuple(name_hash(f"aPosition.{axis}") for axis in "xyz")
FIELD_A_VECTOR = tuple(name_hash(f"aVector.{axis}") for axis in "xyz")
FIELD_A_SOURCE = tuple(name_hash(f"aSource.{axis}") for axis in "xyz")
FIELD_A_TARGET = tuple(name_hash(f"aTarget.{axis}") for axis in "xyz")
FIELD_A_TRACK_FACTOR = name_hash("aTrackFactor")
FIELD_A_UNIT = name_hash("aUnit")
FIELD_AN_UPGRADE_LEVEL = name_hash("anUpgradeLevel")
FIELD_A_TYPE = name_hash("aType")
FIELD_A_HEADING = name_hash("aHeading")
FIELD_A_TIME_TO_LIVE = name_hash("aTimeToLive")
FIELD_A_TEAM = name_hash("aTeam")
FIELD_A_SPAWN_SOURCE = name_hash("aSpawnSource")

WIRE_SHAPES = {
    "straight": (
        (FIELD_AN_ID, TYPE_UINT32),
        (FIELD_A_SUPPORT_THING, TYPE_INT32),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_POSITION),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_VECTOR),
        (FIELD_AN_UPGRADE_LEVEL, TYPE_UINT32),
    ),
    "ballistic": (
        (FIELD_AN_ID, TYPE_UINT32),
        (FIELD_A_SUPPORT_THING, TYPE_INT32),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_POSITION),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_VECTOR),
        (FIELD_AN_UPGRADE_LEVEL, TYPE_UINT32),
    ),
    "homingPosition": (
        (FIELD_A_PROJECTILE, TYPE_UINT32),
        (FIELD_A_SUPPORT_THING, TYPE_INT32),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_SOURCE),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_VECTOR),
        (FIELD_A_TRACK_FACTOR, TYPE_FLOAT32),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_TARGET),
        (FIELD_AN_UPGRADE_LEVEL, TYPE_UINT32),
    ),
    "homingUnit": (
        (FIELD_A_PROJECTILE, TYPE_UINT32),
        (FIELD_A_SUPPORT_THING, TYPE_INT32),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_SOURCE),
        *((field_hash, TYPE_FLOAT32) for field_hash in FIELD_A_VECTOR),
        (FIELD_A_TRACK_FACTOR, TYPE_FLOAT32),
        (FIELD_A_UNIT, TYPE_UINT32),
        (FIELD_AN_UPGRADE_LEVEL, TYPE_UINT32),
    ),
}

STATUS_VALUES = ("exact", "ambiguous", "unmatched", "unsupported", "malformed")


@dataclass(frozen=True)
class ProjectileCreation:
    family: str
    projectile_id: int
    creation_offset: int
    raw_time: float
    support_id: int
    source: tuple[float, float, float]
    vector: tuple[float, float, float]
    upgrade_level: int
    track_factor: float | None = None
    target_position: tuple[float, float, float] | None = None
    target_unit_id: int | None = None

    @property
    def lifecycle_key(self) -> tuple[int, int]:
        return self.projectile_id, self.creation_offset


def _f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def _f32_add(left: float, right: float) -> float:
    return _f32(_f32(left) + _f32(right))


def _f32_mul(left: float, right: float) -> float:
    return _f32(_f32(left) * _f32(right))


def simulate_straight_free_flight(
    source: tuple[float, float, float],
    velocity: tuple[float, float, float],
    tick_deltas: tuple[float, ...],
) -> tuple[tuple[float, float, float], tuple[float, float, float]]:
    """Reproduce b35's float32 straight-mover steps for supplied tick deltas."""
    position = tuple(_f32(value) for value in source)
    current_velocity = tuple(_f32(value) for value in velocity)
    for delta in tick_deltas:
        dt = _f32(delta)
        position = tuple(
            _f32_add(value, _f32_mul(speed, dt))
            for value, speed in zip(position, current_velocity, strict=True)
        )
    return position, current_velocity


def simulate_ballistic_free_flight(
    source: tuple[float, float, float],
    velocity: tuple[float, float, float],
    tick_deltas: tuple[float, ...],
) -> tuple[tuple[float, float, float], tuple[float, float, float]]:
    """Reproduce b35's semi-implicit-order ballistic free-flight updates."""
    position = tuple(_f32(value) for value in source)
    current_velocity = tuple(_f32(value) for value in velocity)
    for delta in tick_deltas:
        dt = _f32(delta)
        position = tuple(
            _f32_add(value, _f32_mul(speed, dt))
            for value, speed in zip(position, current_velocity, strict=True)
        )
        current_velocity = (
            current_velocity[0],
            _f32_add(current_velocity[1], -_f32_mul(GRAVITY, dt)),
            current_velocity[2],
        )
    return position, current_velocity


def decode_support_projectile(data: bytes, envelope) -> ProjectileCreation:
    """Decode one support creation only when its complete wire shape matches."""
    family = SUPPORT_FAMILIES.get(envelope.message)
    if family is None:
        raise ValueError("envelope is not a support projectile creation")
    body, trailing = fields(data, envelope)
    expected = WIRE_SHAPES[family]
    actual = tuple((field.hash, field.flag) for field in body)
    if trailing or actual != expected:
        raise ValueError(
            f"{family} wire mismatch: fields={actual!r}, trailingBytes={len(trailing)}"
        )
    if family in {"straight", "ballistic"}:
        source = tuple(field.f32 for field in body[2:5])
        vector = tuple(field.f32 for field in body[5:8])
        return ProjectileCreation(
            family=family,
            projectile_id=body[0].u32,
            creation_offset=envelope.offset,
            raw_time=envelope.time,
            support_id=body[1].u32,
            source=source,
            vector=vector,
            upgrade_level=body[8].u32,
        )
    creation = ProjectileCreation(
        family=family,
        projectile_id=body[0].u32,
        creation_offset=envelope.offset,
        raw_time=envelope.time,
        support_id=body[1].u32,
        source=tuple(field.f32 for field in body[2:5]),
        vector=tuple(field.f32 for field in body[5:8]),
        track_factor=body[8].f32,
        target_position=(
            tuple(field.f32 for field in body[9:12])
            if family == "homingPosition"
            else None
        ),
        target_unit_id=body[9].u32 if family == "homingUnit" else None,
        upgrade_level=body[12].u32 if family == "homingPosition" else body[10].u32,
    )
    return creation


def primary_chain_end_offset(data: bytes) -> int:
    previous_time = None
    for envelope in walk(data):
        if (
            previous_time is not None
            and previous_time - envelope.time > RECORDING_RESET_DROP_SECONDS
        ):
            return envelope.offset
        previous_time = envelope.time
    return len(data)


def _embedded_u32(data: bytes, start: int, end: int, field_hash: int) -> int | None:
    """Read a field that may follow BinTag's one-byte boolean separator variant."""
    marker = struct.pack("<I", field_hash)
    cursor = start
    while True:
        cursor = data.find(marker, cursor, end)
        if cursor < 0:
            return None
        if cursor + 17 <= end:
            return struct.unpack_from("<I", data, cursor + 13)[0]
        cursor += 1


def _classify_creation(
    creation: ProjectileCreation,
    catalogue_movers: dict[int, tuple[str, ...]],
) -> tuple[str, str]:
    movers = catalogue_movers.get(creation.support_id)
    if movers is None:
        return "unmatched", "supportIdAbsentFromShippedCatalogue"
    if len(movers) != 1:
        return "ambiguous", "multipleShippedDefinitionsForSupportId"
    if movers[0] != EXPECTED_MOVER[creation.family]:
        return "unmatched", "wireFamilyContradictsShippedMoverKind"
    if creation.family in {"homingPosition", "homingUnit"}:
        return "unsupported", "missingTickDeltasDynamicTargetAndCollisionState"
    return "unsupported", "missingTickDeltasAndCollisionState"


def analyse_replay(path: str, catalogue_movers: dict[int, tuple[str, ...]]) -> dict:
    data = decompress(path)
    primary_end = primary_chain_end_offset(data)
    status = Counter()
    reasons = Counter()
    support_families = Counter()
    support_types = Counter()
    normal_families = Counter()
    support_ids = Counter()
    normal_ids = Counter()
    effect_records = Counter()
    malformed = Counter()

    for envelope in walk(data):
        if envelope.offset >= primary_end:
            break
        if envelope.message in SUPPORT_FAMILIES:
            family = SUPPORT_FAMILIES[envelope.message]
            support_families[family] += 1
            try:
                creation = decode_support_projectile(data, envelope)
            except ValueError:
                status["malformed"] += 1
                malformed[family] += 1
                continue
            support_ids[creation.projectile_id] += 1
            support_types[creation.support_id] += 1
            result, reason = _classify_creation(creation, catalogue_movers)
            status[result] += 1
            reasons[reason] += 1
            continue
        if envelope.message in NORMAL_FAMILIES:
            body, trailing = fields(data, envelope)
            normal_families[NORMAL_FAMILIES[envelope.message]] += 1
            if not body or trailing:
                malformed["normalProjectile"] += 1
            else:
                normal_ids[body[0].u32] += 1
            continue
        if envelope.message == MSG_CREATE_CLOUD:
            body, trailing = fields(data, envelope)
            expected_hashes = (
                FIELD_A_TYPE,
                *FIELD_A_POSITION,
                FIELD_A_HEADING,
                FIELD_A_TIME_TO_LIVE,
                FIELD_A_TEAM,
            )
            if not trailing and tuple(field.hash for field in body) == expected_hashes:
                effect_records["CreateCloud"] += 1
            else:
                malformed["CreateCloud"] += 1
            continue
        if envelope.message in {MSG_SPAWN_EXPLOSION, MSG_SPAWN_EXPLOSION_WITH_CRATER}:
            body, trailing = fields(data, envelope)
            minimum = 8 if envelope.message == MSG_SPAWN_EXPLOSION_WITH_CRATER else 7
            name = (
                "SpawnExplosionWithCrater"
                if envelope.message == MSG_SPAWN_EXPLOSION_WITH_CRATER
                else "SpawnExplosion"
            )
            if not trailing and len(body) == minimum:
                effect_records[name] += 1
            else:
                malformed[name] += 1
            continue
        if envelope.message == MSG_UNIT_CREATE:
            spawn_source = _embedded_u32(
                data, envelope.body_start, envelope.body_end, FIELD_A_SPAWN_SOURCE
            )
            if spawn_source == 1:
                effect_records["UnitCreateSpawnSource1"] += 1

    overlap = set(support_ids) & set(normal_ids)
    return {
        "path": path,
        "status": dict(status),
        "reasons": dict(reasons),
        "supportFamilies": dict(support_families),
        "supportTypes": {f"0x{key:08x}": value for key, value in support_types.items()},
        "normalFamilies": dict(normal_families),
        "effectRecords": dict(effect_records),
        "malformed": dict(malformed),
        "supportCreations": sum(support_ids.values()),
        "supportDistinctIds": len(support_ids),
        "supportIdReuseCreations": sum(value - 1 for value in support_ids.values()),
        "normalCreations": sum(normal_ids.values()),
        "normalDistinctIds": len(normal_ids),
        "normalIdReuseCreations": sum(value - 1 for value in normal_ids.values()),
        "crossFamilyIdOverlap": len(overlap),
        "endSummaryPassOmitted": primary_end < len(data),
    }


def analyse_replay_safe(
    path: str, catalogue_movers: dict[int, tuple[str, ...]]
) -> dict:
    try:
        return analyse_replay(path, catalogue_movers)
    except Exception as error:  # Keep a complete corpus ledger for corrupt inputs.
        return {
            "path": path,
            "errorType": type(error).__name__,
            "error": str(error),
        }


def discover_replays(inputs: list[str]) -> list[str]:
    """Resolve files/directories, follow corpus symlinks, and deduplicate paths."""
    discovered: dict[pathlib.Path, str] = {}
    for raw in inputs:
        path = pathlib.Path(raw)
        if path.is_file():
            resolved = path.resolve()
            discovered.setdefault(resolved, str(path))
            continue
        if not path.is_dir():
            raise FileNotFoundError(raw)
        for root, _directories, files in os.walk(path, followlinks=True):
            for filename in files:
                if not filename.lower().endswith(".wicdemo"):
                    continue
                candidate = pathlib.Path(root, filename)
                discovered.setdefault(candidate.resolve(), str(candidate))
    return [discovered[key] for key in sorted(discovered, key=str)]


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def _catalogue_json(
    definitions: dict[int, tuple[SupportProjectileDefinition, ...]],
) -> list[dict]:
    rows = []
    for support_id, variants in sorted(definitions.items()):
        for variant_index, definition in enumerate(variants):
            row = asdict(definition)
            row["support_id"] = support_id
            row["support_id_hex"] = f"0x{support_id:08x}"
            row["variant_index"] = variant_index
            row["variant_count"] = len(variants)
            for effect in row["effects"]:
                effect["type_hash_hex"] = f"0x{effect['type_hash']:08x}"
                effect["key_hash_hex"] = f"0x{effect['key_hash']:08x}"
                effect["scalar_fields"] = [
                    {"fieldHash": f"0x{field_hash:08x}", "value": value}
                    for field_hash, value in effect["scalar_fields"]
                ]
            rows.append(row)
    return rows


def build_report(
    replay_paths: list[str],
    archive_path: pathlib.Path,
    jobs: int,
) -> dict:
    definitions = shipped_support_projectile_definitions(archive_path)
    movers = {
        support_id: tuple(definition.mover_kind for definition in variants)
        for support_id, variants in definitions.items()
    }
    if jobs == 1:
        replay_rows = [analyse_replay_safe(path, movers) for path in replay_paths]
    else:
        # Python 3.14 defaults to forkserver on this Fedora host; its Unix socket
        # is unavailable in the managed analysis sandbox.  These workers are
        # read-only and start before any threads, so an explicit fork context is
        # both deterministic and compatible here.
        with concurrent.futures.ProcessPoolExecutor(
            max_workers=jobs, mp_context=multiprocessing.get_context("fork")
        ) as executor:
            replay_rows = list(
                executor.map(
                    analyse_replay_safe,
                    replay_paths,
                    (movers for _ in replay_paths),
                    chunksize=1,
                )
            )

    totals = Counter()
    status = Counter()
    reasons = Counter()
    support_families = Counter()
    normal_families = Counter()
    support_types = Counter()
    effects = Counter()
    malformed = Counter()
    failures = []
    wire_shape_failures = []
    for row in replay_rows:
        if "error" in row:
            failures.append(row)
            continue
        status.update(row["status"])
        reasons.update(row["reasons"])
        support_families.update(row["supportFamilies"])
        normal_families.update(row["normalFamilies"])
        support_types.update(row["supportTypes"])
        effects.update(row["effectRecords"])
        malformed.update(row["malformed"])
        for key in (
            "supportCreations",
            "supportDistinctIds",
            "supportIdReuseCreations",
            "normalCreations",
            "normalDistinctIds",
            "normalIdReuseCreations",
            "crossFamilyIdOverlap",
        ):
            totals[key] += row[key]
        if row["malformed"]:
            wire_shape_failures.append(
                {"path": row["path"], "malformed": row["malformed"]}
            )

    for value in STATUS_VALUES:
        status.setdefault(value, 0)
    all_variants = [
        definition for variants in definitions.values() for definition in variants
    ]
    catalogue_movers = Counter(definition.mover_kind for definition in all_variants)
    active_movers = Counter(
        definition.mover_kind
        for definition in all_variants
        if definition.projectile_count > 0
    )
    effect_types = Counter(
        effect.type_name or f"0x{effect.type_hash:08x}"
        for definition in all_variants
        for effect in definition.effects
    )
    archive_hash = _sha256(archive_path)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "scope": "researchOnlyNoUnitDestroyOrDeathAttribution",
        "exactnessPolicy": "exactOrAbstainNoTimeDistanceOrProximityThresholds",
        "sourceEvidence": {
            "clientExecutable": {
                "path": "local/binaries/client/wic.exe",
                "sha256": CLIENT_EXE_SHA256,
                "architecture": "PE32 i386",
                "supportSerializers": [
                    "0x00b86340",
                    "0x00b86250",
                    "0x00b85fc0",
                    "0x00b86100",
                ],
            },
            "serverExecutable": {
                "path": "local/binaries/server/wic_ds.exe",
                "sha256": SERVER_EXE_SHA256,
                "architecture": "PE32 i386",
                "projectileUpdate": "0x00520450",
                "straightMoverUpdate": "0x00683970",
                "ballisticMoverUpdate": "0x00683a20",
                "homingMoverUpdate": "0x00684a00",
                "moverAndWorldCollision": "0x006837d0",
                "deathParasites": "0x00520010",
                "timeUpdate": "0x0041b150",
            },
            "serverArchive": {
                "path": str(archive_path),
                "sha256": archive_hash,
                "expectedSha256": SERVER_SDF_SHA256,
                "hashMatchesExpected": archive_hash == SERVER_SDF_SHA256,
            },
        },
        "simulationContract": {
            "lifecycleKey": ["projectileId", "creationOffset"],
            "straight": "float32 position += velocity * suppliedTickDelta",
            "ballistic": (
                "float32 position += velocity * suppliedTickDelta, then "
                "velocityY -= 9.81 * suppliedTickDelta"
            ),
            "homing": "not reproduced without dynamic target and per-tick world state",
            "collision": "not reproduced without map collision/world query inputs",
            "replayBlockers": [
                "per-frame tick delta sequence is not serialized",
                "collision/world-query result is not serialized",
                "effect records carry no projectile ID",
            ],
            "trajectoryBounds": (
                "none emitted: no binary-derived finite bound exists without the "
                "missing delta count and collision geometry"
            ),
        },
        "catalogueSummary": {
            "uniqueDefinitions": len(definitions),
            "serializedVariants": len(all_variants),
            "ambiguousSupportIds": [
                f"0x{support_id:08x}"
                for support_id, variants in sorted(definitions.items())
                if len(variants) > 1
            ],
            "projectileProducingDefinitions": sum(
                definition.projectile_count > 0 for definition in all_variants
            ),
            "allDefinitionsByMover": dict(catalogue_movers),
            "projectileProducingByMover": dict(active_movers),
            "effectNodesByVerifiedType": dict(effect_types),
        },
        "catalogue": _catalogue_json(definitions),
        "corpus": {
            "replayInputs": len(replay_rows),
            "replaysAnalysed": len(replay_rows) - len(failures),
            "failures": failures,
            "wireShapeFailures": wire_shape_failures,
            "status": dict(status),
            "abstentionReasons": dict(reasons),
            "supportFamilies": dict(support_families),
            "normalControlFamilies": dict(normal_families),
            "supportTypes": dict(sorted(support_types.items())),
            "effectRecordInventory": dict(effects),
            "malformed": dict(malformed),
            **dict(totals),
            "effectLinks": {
                "exact": 0,
                "ambiguous": 0,
                "basis": "no serialized projectileId bridge on effect records",
            },
        },
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "inputs",
        nargs="*",
        default=["replays"],
        help="replay files or directories (default: replays)",
    )
    parser.add_argument(
        "--archive",
        type=pathlib.Path,
        default=pathlib.Path("local/binaries/server/wic_ds.sdf"),
    )
    parser.add_argument("--json", type=pathlib.Path)
    parser.add_argument(
        "--jobs",
        type=int,
        default=max(1, min(multiprocessing.cpu_count(), 8)),
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.jobs < 1:
        raise SystemExit("--jobs must be at least 1")
    paths = discover_replays(args.inputs)
    report = build_report(paths, args.archive, args.jobs)
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.json is None:
        print(encoded, end="")
    else:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(encoded)


if __name__ == "__main__":
    main()
