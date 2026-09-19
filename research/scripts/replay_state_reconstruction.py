#!/usr/bin/env python3
"""Decode and audit state-faithful 2D data from World in Conflict replays.

This is a research interface, deliberately separate from the canonical parser's
lightweight timeline.  It retains the raw packed UnitFrame words and envelope
offsets so every displayed checkpoint can be traced back to replay bytes.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import concurrent.futures
import hashlib
import json
import math
import pathlib
import struct
import zlib
from collections import Counter
from dataclasses import dataclass
from typing import Any, Iterable

from wic_bintag import Envelope, decompress, name_hash, walk
from wic_sdf import SdfArchive

SCHEMA_VERSION = 1
RECORDING_RESET_DROP_SECONDS = 1.0
DDS_HEADER_SIZE = 128
ICE_SIGNATURE = b"ice0010\0"
TYPE_VECTOR2 = 0x0AE102A6
TYPE_REAL = 0x07A901F0
MIN_COORD = 0x4CB1079C
MAX_COORD = 0x4CB5079E
X_COORD = 0x00790079
Z_COORD = 0x007B007B
POSITION_DECODE_FACTOR = struct.unpack("<f", struct.pack("<f", 0.023809524))[0]
ORIENTATION_DECODE_FACTOR = struct.unpack("<f", struct.pack("<f", 0.0002))[0]

MSG_ADD_COMMAND_POINT = name_hash("AddCommandPoint")
MSG_ADD_PERIMETER_POINT = name_hash("AddPerimeterPoint")
MSG_SET_COMMAND_POINT_OWNER = name_hash("SetCommandPointOwner")
MSG_SET_PERIMETER_POINT_OWNER = name_hash("SetPerimeterPointOwner")
MSG_UNIT_CREATE = name_hash("UnitCreate")
MSG_UNIT_FRAME = name_hash("UnitFrame")
MSG_UNIT_HEALTH = name_hash("UnitHealth")
MSG_UNIT_DESTROY = name_hash("UnitDestroy")
MSG_UNIT_REMOVE = name_hash("UnitRemove")
MSG_UNIT_SET_OWNER = name_hash("UnitSetOwner")
MSG_UNIT_SET_TEAM = name_hash("UnitSetTeam")
MSG_MOVER_IS_MOVING = name_hash("MoverIsMoving")
MSG_MOVER_QUEUED = name_hash("MoverQueued")
MSG_MOVER_HALTED = name_hash("MoverHalted")
MSG_BROADCAST_LONG_MOVE = name_hash("BroadcastLongMove")
MSG_CLEAR_ORDER_QUEUE = name_hash("ClearOrderQueue")
MSG_SUPPORT_DEPLOYED = name_hash("SupportThingSpawnedDelayed")
MSG_SUPPORT_MARKER = name_hash("SupportThingMarker")
MSG_TEAM_WINS = name_hash("TeamWins")

FIELD_UNIT_FRAME_COMPACT = name_hash("UnitFrameDataCompact")
FIELD_UNIT_FRAME_FULL = name_hash("UnitFrameData")
FIELD_SEP = b"\x11\x00\x00\x00"


def _field(name: str) -> int:
    return name_hash(name)


FIELD_NAMES = {
    name: _field(name)
    for name in (
        "aName",
        "aParent",
        "aPlayer",
        "playerId",
        "aTeam",
        "aUnit",
        "aType",
        "aMover",
        "aKiller",
        "aHealth",
        "aHeading",
        "aSpeed",
        "aPosition.x",
        "aPosition.y",
        "aPosition.z",
        "aDirection.x",
        "aDirection.y",
        "aDirection.z",
        "anId",
        "anEventId",
        "anIsCapturable",
        "aRadius",
        "aCurrentHealth",
        "aCurrentExperience",
        "aCurrentFireBehaviour",
        "aNumShooters",
        "aSupportUppgradeLevel",
        "aTimeSinceCreation",
        "aIsToBeReplacedBySpecialistFlag",
    )
}


@dataclass(frozen=True)
class RawField:
    flag: int
    raw: bytes

    @property
    def u32(self) -> int:
        return struct.unpack("<I", self.raw)[0]

    @property
    def i32(self) -> int:
        return struct.unpack("<i", self.raw)[0]

    @property
    def f32(self) -> float:
        return struct.unpack("<f", self.raw)[0]

    @property
    def boolean(self) -> bool:
        return self.raw[0] != 0


def _field_in(data: bytes, envelope: Envelope, name: str) -> RawField | None:
    """Find one structurally valid scalar field inside a bounded envelope."""
    wanted = struct.pack("<I", FIELD_NAMES[name])
    body = data[envelope.body_start : envelope.body_end]
    found: RawField | None = None
    start = 0
    while True:
        relative = body.find(wanted, start)
        if relative < 0:
            return found
        absolute = envelope.body_start + relative
        if (
            absolute + 17 <= envelope.body_end
            and data[absolute + 4 : absolute + 8] == FIELD_SEP
            and data[absolute + 9 : absolute + 13] == FIELD_SEP
        ):
            candidate = RawField(
                flag=data[absolute + 8], raw=data[absolute + 13 : absolute + 17]
            )
            if found is not None:
                return None
            found = candidate
        start = relative + 1


def _u32(data: bytes, envelope: Envelope, name: str) -> int | None:
    value = _field_in(data, envelope, name)
    return value.u32 if value is not None else None


def _f32(data: bytes, envelope: Envelope, name: str) -> float | None:
    value = _field_in(data, envelope, name)
    if value is None:
        return None
    result = value.f32
    return result if math.isfinite(result) else None


def _bool(data: bytes, envelope: Envelope, name: str) -> bool | None:
    value = _field_in(data, envelope, name)
    return value.boolean if value is not None else None


def _vector(data: bytes, envelope: Envelope, base: str) -> list[float] | None:
    values = [_f32(data, envelope, f"{base}.{axis}") for axis in "xyz"]
    return (
        [float(value) for value in values]
        if all(v is not None for v in values)
        else None
    )


def _primary_envelopes(data: bytes) -> Iterable[Envelope]:
    previous: float | None = None
    for envelope in walk(data):
        if (
            previous is not None
            and previous - envelope.time > RECORDING_RESET_DROP_SECONDS
        ):
            return
        previous = envelope.time
        yield envelope


def _map_name(data: bytes) -> str:
    if len(data) <= 47:
        return "Unknown"
    end = data.find(b"\0", 47, min(len(data), 47 + 512))
    if end < 0:
        return "Unknown"
    return data[47:end].decode("utf-8", errors="replace") or "Unknown"


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _float_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def _bits_float(value: int) -> float:
    return struct.unpack("<f", struct.pack("<I", value))[0]


def _float32(value: float) -> float:
    return _bits_float(_float_bits(value))


def decode_compact_position(value: int) -> float:
    return _float32(_float32(float(value) - 0.5) * POSITION_DECODE_FACTOR)


def decode_compact_orientation(value: int) -> float:
    return _float32(_float32(float(value) - 20000.5) * ORIENTATION_DECODE_FACTOR)


def _png_chunk(kind: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", binascii.crc32(kind + payload) & 0xFFFFFFFF)
    )


def _rgb565(value: int) -> tuple[int, int, int]:
    red = (value >> 11) & 0x1F
    green = (value >> 5) & 0x3F
    blue = value & 0x1F
    return (red * 255 // 31, green * 255 // 63, blue * 255 // 31)


def dxt1_dds_to_png(data: bytes) -> bytes:
    """Decode the validated top DXT1 mip and return a dependency-free RGB PNG."""
    if len(data) < DDS_HEADER_SIZE or data[:4] != b"DDS ":
        raise ValueError("overview art is not a DDS file")
    height, width = struct.unpack_from("<II", data, 12)
    if data[84:88] != b"DXT1" or width <= 0 or height <= 0:
        raise ValueError("overview art is not a bounded DXT1 image")
    payload_size = max(1, (width + 3) // 4) * max(1, (height + 3) // 4) * 8
    payload = data[DDS_HEADER_SIZE : DDS_HEADER_SIZE + payload_size]
    if len(payload) != payload_size:
        raise ValueError("overview DXT1 payload is truncated")
    pixels = bytearray(width * height * 3)
    cursor = 0
    for block_y in range((height + 3) // 4):
        for block_x in range((width + 3) // 4):
            color0, color1, indices = struct.unpack_from("<HHI", payload, cursor)
            cursor += 8
            first, second = _rgb565(color0), _rgb565(color1)
            if color0 > color1:
                palette = (
                    first,
                    second,
                    tuple((2 * first[i] + second[i]) // 3 for i in range(3)),
                    tuple((first[i] + 2 * second[i]) // 3 for i in range(3)),
                )
            else:
                palette = (
                    first,
                    second,
                    tuple((first[i] + second[i]) // 2 for i in range(3)),
                    (0, 0, 0),
                )
            for local_y in range(4):
                for local_x in range(4):
                    x = block_x * 4 + local_x
                    y = block_y * 4 + local_y
                    palette_index = (indices >> (2 * (local_y * 4 + local_x))) & 3
                    if x < width and y < height:
                        target = (y * width + x) * 3
                        pixels[target : target + 3] = bytes(palette[palette_index])
    scanlines = b"".join(
        b"\0" + pixels[row * width * 3 : (row + 1) * width * 3] for row in range(height)
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + _png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + _png_chunk(b"IDAT", zlib.compress(scanlines, 9))
        + _png_chunk(b"IEND", b"")
    )


def _read_ice_real(data: bytes, cursor: int, component: int) -> tuple[float, int]:
    if cursor + 16 > len(data):
        raise ValueError("truncated playfield coordinate")
    kind, name, array_index, length = struct.unpack_from("<IIII", data, cursor)
    if kind != TYPE_REAL or name != component or array_index != 0xFFFFFFFF:
        raise ValueError("unexpected playfield coordinate layout")
    if length <= 0 or length > 64 or cursor + 16 + length > len(data):
        raise ValueError("invalid playfield coordinate length")
    value = float(data[cursor + 16 : cursor + 16 + length].decode("utf-8"))
    if not math.isfinite(value):
        raise ValueError("non-finite playfield coordinate")
    return value, cursor + 16 + length


def _ice_vector2(data: bytes, field: int) -> tuple[float, float]:
    marker = struct.pack("<III", TYPE_VECTOR2, field, 2)
    matches = [
        index + len(marker)
        for index in range(len(data))
        if data.startswith(marker, index)
    ]
    if len(matches) != 1:
        raise ValueError(f"expected one playfield vector, found {len(matches)}")
    x, cursor = _read_ice_real(data, matches[0], X_COORD)
    z, _ = _read_ice_real(data, cursor, Z_COORD)
    return x, z


def playfield_bounds(data: bytes) -> dict[str, float]:
    if not data.startswith(ICE_SIGNATURE):
        raise ValueError("map definition is not an ICE file")
    minimum = _ice_vector2(data, MIN_COORD)
    maximum = _ice_vector2(data, MAX_COORD)
    if maximum[0] <= minimum[0] or maximum[1] <= minimum[1]:
        raise ValueError("map playfield bounds are inverted")
    return {
        "minX": minimum[0],
        "minZ": minimum[1],
        "maxX": maximum[0],
        "maxZ": maximum[1],
    }


def load_map_asset(
    map_name: str, archives: list[pathlib.Path]
) -> dict[str, Any] | None:
    """Load one map from the first explicitly ordered archive that contains it."""
    internal = pathlib.PurePosixPath(map_name.replace("\\", "/")).stem.lower()
    if not internal or internal == "unknown":
        return None
    ice_path = f"maps/{internal}/{internal}.ice"
    overview_path = f"maps/{internal}/overviewmap.dds"
    for archive_path in archives:
        archive = SdfArchive.open(archive_path)
        by_path = {entry.path.lower(): entry for entry in archive.entries}
        ice_entry = by_path.get(ice_path)
        art_entry = by_path.get(overview_path)
        if ice_entry is None or art_entry is None:
            continue
        ice = archive.read_entry(ice_entry)
        dds = archive.read_entry(art_entry)
        png = dxt1_dds_to_png(dds)
        return {
            "bounds": playfield_bounds(ice),
            "overviewPngDataUrl": "data:image/png;base64,"
            + base64.b64encode(png).decode("ascii"),
            "sourceArchive": str(archive_path),
            "sourceArchiveSha256": _sha256(archive_path),
            "iceEntry": ice_entry.path,
            "iceSha256": hashlib.sha256(ice).hexdigest(),
            "overviewEntry": art_entry.path,
            "overviewDdsSha256": hashlib.sha256(dds).hexdigest(),
        }
    raise ValueError(
        f"map {internal!r} was not found in the explicitly supplied archives"
    )


def _decode_compact_words(words: list[int]) -> dict[str, Any]:
    header = words[0]
    child_count = (header >> 12) & 7
    children = []
    for index in range(child_count):
        cursor = 8 + index * 3
        tag = words[cursor]
        children.append(
            {
                "index": (tag & 0x7F) - 1,
                "flag": bool(tag & 0x80),
                "orientation": [
                    decode_compact_orientation(words[cursor + 1]),
                    decode_compact_orientation(words[cursor + 2]),
                ],
            }
        )
    return {
        "unitId": header & 0x0FFF,
        "childCount": child_count,
        "position": [decode_compact_position(value) for value in words[1:4]],
        "orientation": [decode_compact_orientation(value) for value in words[4:8]],
        "children": children,
    }


def decode_unit_frame(data: bytes, envelope: Envelope) -> dict[str, Any] | None:
    """Decode one full or compact UnitFrame while retaining its raw words."""
    body = data[envelope.body_start : envelope.body_end]
    if len(body) < 13:
        return None
    field_hash, total = struct.unpack_from("<II", body)
    if body[8] != 6 or struct.unpack_from("<I", body, 9)[0] != total:
        return None
    if total != len(body):
        return None
    payload = body[13:]
    common = {"timeBits": _float_bits(envelope.time), "offset": envelope.offset}
    if field_hash == FIELD_UNIT_FRAME_COMPACT:
        if len(payload) < 16 or (len(payload) - 16) % 5:
            return None
        header = struct.unpack_from("<H", payload)[0]
        child_count = (header >> 12) & 7
        if len(payload) != 16 + child_count * 5:
            return None
        base_words = list(struct.unpack_from("<8H", payload))
        child_words: list[int] = []
        cursor = 16
        for _ in range(child_count):
            tag = payload[cursor]
            first, second = struct.unpack_from("<HH", payload, cursor + 1)
            child_words.extend((tag, first, second))
            cursor += 5
        raw = base_words + child_words
        return {
            **common,
            "encoding": "compact",
            "raw": raw,
            **_decode_compact_words(raw),
        }
    if field_hash == FIELD_UNIT_FRAME_FULL:
        if len(payload) < 31:
            return None
        child_count = payload[30]
        if len(payload) != 31 + child_count * 10:
            return None
        parent_bits = list(struct.unpack_from("<7I", payload))
        parent_values = [_bits_float(value) for value in parent_bits]
        if not all(math.isfinite(value) for value in parent_values):
            return None
        unit_id = struct.unpack_from("<H", payload, 28)[0]
        children = []
        raw_children = []
        cursor = 31
        for _ in range(child_count):
            first_bits, second_bits = struct.unpack_from("<II", payload, cursor)
            flag = payload[cursor + 8]
            child_index = payload[cursor + 9]
            raw_children.append([first_bits, second_bits, flag, child_index])
            children.append(
                {
                    "index": child_index,
                    "flag": bool(flag),
                    "orientation": [_bits_float(first_bits), _bits_float(second_bits)],
                }
            )
            cursor += 10
        return {
            **common,
            "encoding": "full",
            "raw": {"parentBits": parent_bits, "children": raw_children},
            "unitId": unit_id,
            "childCount": child_count,
            "position": parent_values[:3],
            "orientation": parent_values[3:],
            "children": children,
        }
    return None


def _event_base(envelope: Envelope, kind: str) -> dict[str, Any]:
    return {
        "type": kind,
        "timeSeconds": envelope.time,
        "timeBits": _float_bits(envelope.time),
        "offset": envelope.offset,
    }


def _percentile(values: list[float], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int((len(ordered) - 1) * fraction))]


def decode_replay(
    path: pathlib.Path,
    include_frames: bool = True,
    map_archives: list[pathlib.Path] | None = None,
) -> dict[str, Any]:
    data = decompress(path)
    if not data:
        raise ValueError("replay contains no decodable zlib streams")

    units: list[dict[str, Any]] = []
    active: dict[int, int] = {}
    generations: Counter[int] = Counter()
    objectives: list[dict[str, Any]] = []
    events: list[dict[str, Any]] = []
    malformed: Counter[str] = Counter()
    counts: Counter[str] = Counter()
    frame_gaps: list[float] = []
    last_frame_time: dict[int, float] = {}
    first_time: float | None = None
    duration = 0.0
    envelope_count = 0
    team_wins = 0

    for envelope in _primary_envelopes(data):
        envelope_count += 1
        first_time = envelope.time if first_time is None else first_time
        duration = max(duration, envelope.time)
        message = envelope.message

        if message == MSG_TEAM_WINS:
            team_wins += 1

        if message == MSG_UNIT_CREATE:
            counts["unitCreate"] += 1
            unit_id = _u32(data, envelope, "aUnit")
            position = _vector(data, envelope, "aPosition")
            if unit_id is None or position is None:
                malformed["unitCreate"] += 1
                continue
            generations[unit_id] += 1
            record = {
                "unitId": unit_id,
                "generation": generations[unit_id],
                "generationKey": f"{unit_id}:{envelope.offset}",
                "creationOffset": envelope.offset,
                "createdSeconds": envelope.time,
                "playerId": _u32(data, envelope, "aPlayer"),
                "team": _u32(data, envelope, "aTeam"),
                "unitTypeId": _u32(data, envelope, "aType"),
                "spawnPosition": position,
                "heading": _f32(data, envelope, "aHeading"),
                "initialHealth": _f32(data, envelope, "aCurrentHealth"),
                "experience": _f32(data, envelope, "aCurrentExperience"),
                "frames": [],
                "health": [],
                "changes": [],
                "terminal": None,
            }
            if unit_id in active:
                units[active[unit_id]]["terminal"] = {
                    **_event_base(envelope, "replacedByCreate"),
                }
            last_frame_time.pop(unit_id, None)
            active[unit_id] = len(units)
            units.append(record)
            continue

        if message == MSG_UNIT_FRAME:
            counts["unitFrame"] += 1
            frame = decode_unit_frame(data, envelope)
            if frame is None:
                malformed["unitFrame"] += 1
                continue
            counts[f"unitFrame.{frame['encoding']}"] += 1
            unit_id = frame["unitId"]
            previous = last_frame_time.get(unit_id)
            if previous is not None and envelope.time >= previous:
                frame_gaps.append(envelope.time - previous)
            last_frame_time[unit_id] = envelope.time
            index = active.get(unit_id)
            if index is None:
                counts["orphanUnitFrame"] += 1
            elif include_frames:
                units[index]["frames"].append(
                    {
                        key: frame[key]
                        for key in ("timeBits", "offset", "encoding", "raw")
                    }
                )
            continue

        if message == MSG_UNIT_HEALTH:
            counts["unitHealth"] += 1
            unit_id = _u32(data, envelope, "aUnit")
            health = _f32(data, envelope, "aHealth")
            if unit_id is None or health is None:
                malformed["unitHealth"] += 1
                continue
            index = active.get(unit_id)
            if index is None:
                counts["orphanUnitHealth"] += 1
            else:
                units[index]["health"].append(
                    {**_event_base(envelope, "health"), "value": health}
                )
            continue

        if message in (MSG_UNIT_SET_OWNER, MSG_UNIT_SET_TEAM):
            kind = "ownerChanged" if message == MSG_UNIT_SET_OWNER else "teamChanged"
            counts[kind] += 1
            unit_id = _u32(data, envelope, "aUnit")
            index = active.get(unit_id) if unit_id is not None else None
            if index is None:
                counts[f"orphan{kind[0].upper()}{kind[1:]}"] += 1
                continue
            value_name = "playerId" if message == MSG_UNIT_SET_OWNER else "aTeam"
            value = _u32(data, envelope, value_name)
            units[index]["changes"].append(
                {**_event_base(envelope, kind), "value": value}
            )
            continue

        if message in (MSG_UNIT_DESTROY, MSG_UNIT_REMOVE):
            kind = "destroyed" if message == MSG_UNIT_DESTROY else "removed"
            counts[f"unit{kind.title()}"] += 1
            unit_id = _u32(data, envelope, "aUnit")
            index = active.pop(unit_id, None) if unit_id is not None else None
            terminal = {
                **_event_base(envelope, kind),
                "killerUnitId": (
                    _u32(data, envelope, "aKiller")
                    if message == MSG_UNIT_DESTROY
                    else None
                ),
                "specialistReplacement": (
                    _bool(data, envelope, "aIsToBeReplacedBySpecialistFlag")
                    if message == MSG_UNIT_REMOVE
                    else None
                ),
            }
            if index is None:
                counts[f"orphanUnit{kind.title()}"] += 1
            else:
                units[index]["terminal"] = terminal
            events.append({**terminal, "unitId": unit_id})
            continue

        if message in (
            MSG_MOVER_IS_MOVING,
            MSG_MOVER_QUEUED,
            MSG_MOVER_HALTED,
            MSG_BROADCAST_LONG_MOVE,
            MSG_CLEAR_ORDER_QUEUE,
        ):
            kinds = {
                MSG_MOVER_IS_MOVING: "moveDestination",
                MSG_MOVER_QUEUED: "queuedMoveDestination",
                MSG_MOVER_HALTED: "moverHalted",
                MSG_BROADCAST_LONG_MOVE: "longMoveBroadcast",
                MSG_CLEAR_ORDER_QUEUE: "orderQueueCleared",
            }
            kind = kinds[message]
            counts[kind] += 1
            unit_id = _u32(data, envelope, "aMover")
            if unit_id is None:
                unit_id = _u32(data, envelope, "aUnit")
            record = {
                **_event_base(envelope, kind),
                "unitId": unit_id,
                "position": _vector(data, envelope, "aPosition"),
                "heading": _f32(data, envelope, "aHeading"),
                "speed": _f32(data, envelope, "aSpeed"),
            }
            index = active.get(unit_id) if unit_id is not None else None
            if index is not None:
                units[index]["changes"].append(record)
            events.append(record)
            continue

        if message in (MSG_ADD_COMMAND_POINT, MSG_ADD_PERIMETER_POINT):
            kind = (
                "commandPoint" if message == MSG_ADD_COMMAND_POINT else "perimeterPoint"
            )
            counts[f"add{kind[0].upper()}{kind[1:]}"] += 1
            record = {
                **_event_base(envelope, kind),
                "id": _u32(data, envelope, "aName"),
                "parentId": _u32(data, envelope, "aParent"),
                "position": _vector(data, envelope, "aPosition"),
                "team": _u32(data, envelope, "aTeam"),
                "capturable": _bool(data, envelope, "anIsCapturable"),
                "radius": _f32(data, envelope, "aRadius"),
            }
            objectives.append(record)
            continue

        if message in (MSG_SET_COMMAND_POINT_OWNER, MSG_SET_PERIMETER_POINT_OWNER):
            kind = (
                "commandPointOwnerChanged"
                if message == MSG_SET_COMMAND_POINT_OWNER
                else "perimeterPointOwnerChanged"
            )
            counts[kind] += 1
            events.append(
                {
                    **_event_base(envelope, kind),
                    "id": _u32(data, envelope, "aName"),
                    "team": _u32(data, envelope, "aTeam"),
                }
            )
            continue

        if message in (MSG_SUPPORT_DEPLOYED, MSG_SUPPORT_MARKER):
            kind = (
                "tacticalAidDeployed"
                if message == MSG_SUPPORT_DEPLOYED
                else "tacticalAidMarker"
            )
            counts[kind] += 1
            events.append(
                {
                    **_event_base(envelope, kind),
                    "supportId": _u32(data, envelope, "anId"),
                    "eventId": _u32(data, envelope, "anEventId"),
                    "position": _vector(data, envelope, "aPosition"),
                    "direction": _vector(data, envelope, "aDirection"),
                    "teamOrPlayerRaw": _u32(data, envelope, "aTeam"),
                    "upgradeLevel": _u32(data, envelope, "aSupportUppgradeLevel"),
                    "ageSeconds": _f32(data, envelope, "aTimeSinceCreation"),
                }
            )

    if envelope_count == 0:
        raise ValueError("replay has no structurally valid Event envelope chain")
    gaps = {
        "count": len(frame_gaps),
        "p50Seconds": _percentile(frame_gaps, 0.50),
        "p90Seconds": _percentile(frame_gaps, 0.90),
        "p99Seconds": _percentile(frame_gaps, 0.99),
        "maxSeconds": max(frame_gaps, default=None),
        "buckets": {
            "lte0.4": sum(value <= 0.4 for value in frame_gaps),
            "lte0.75": sum(0.4 < value <= 0.75 for value in frame_gaps),
            "lte1.5": sum(0.75 < value <= 1.5 for value in frame_gaps),
            "lte5": sum(1.5 < value <= 5.0 for value in frame_gaps),
            "gt5": sum(value > 5.0 for value in frame_gaps),
        },
    }
    events.sort(key=lambda row: (row["timeSeconds"], row["offset"]))
    map_name = _map_name(data)
    map_asset = load_map_asset(map_name, map_archives) if map_archives else None
    return {
        "schemaVersion": SCHEMA_VERSION,
        "contract": "researchReplayState",
        "source": {
            "replaySha256": _sha256(path),
            "fileName": path.name,
            "mapName": map_name,
        },
        "timing": {
            "recordingStartSeconds": first_time or 0.0,
            "recordingSeconds": duration,
            "axis": "eventEnvelopeRecordingTime",
        },
        "map": {
            "bounds": map_asset["bounds"] if map_asset else None,
            "overviewPngDataUrl": map_asset["overviewPngDataUrl"]
            if map_asset
            else None,
            "assetProvenance": (
                {
                    key: value
                    for key, value in map_asset.items()
                    if key not in {"bounds", "overviewPngDataUrl"}
                }
                if map_asset
                else None
            ),
            "objectives": objectives,
        },
        "units": units,
        "events": events,
        "coverage": {
            "counts": dict(sorted(counts.items())),
            "malformed": dict(sorted(malformed.items())),
            "frameGaps": gaps,
            "positionSemantics": "authoritativeSerializedCheckpoints",
            "betweenFrameSemantics": "unprovenHoldLast",
            "pointOfView": "notYetClassified",
            "teamWinsEvents": team_wins,
        },
    }


def _discover(roots: Iterable[pathlib.Path]) -> list[pathlib.Path]:
    paths: set[pathlib.Path] = set()
    for root in roots:
        if root.is_file() and root.suffix.lower() == ".wicdemo":
            paths.add(root)
        elif root.is_dir():
            paths.update(root.rglob("*.wicdemo"))
    return sorted(paths, key=lambda path: str(path))


def _audit_one(path_text: str) -> dict[str, Any]:
    path = pathlib.Path(path_text)
    try:
        document = decode_replay(path, include_frames=False)
    except Exception as error:  # noqa: BLE001 - corpus audit must retain every failure
        return {
            "path": str(path),
            "error": f"{type(error).__name__}: {error}",
            "sha256": _sha256(path),
        }
    return {
        "path": str(path),
        "sha256": document["source"]["replaySha256"],
        "mapName": document["source"]["mapName"],
        "recordingSeconds": document["timing"]["recordingSeconds"],
        "counts": document["coverage"]["counts"],
        "malformed": document["coverage"]["malformed"],
        "frameGaps": document["coverage"]["frameGaps"],
        "teamWinsEvents": document["coverage"]["teamWinsEvents"],
    }


def audit_replays(paths: list[pathlib.Path], jobs: int) -> dict[str, Any]:
    workers = max(1, jobs)
    if workers == 1:
        rows = [_audit_one(str(path)) for path in paths]
    else:
        with concurrent.futures.ProcessPoolExecutor(max_workers=workers) as pool:
            rows = list(pool.map(_audit_one, map(str, paths)))
    counts: Counter[str] = Counter()
    malformed: Counter[str] = Counter()
    gap_buckets: Counter[str] = Counter()
    gap_max = 0.0
    without_team_wins = 0
    errors = []
    for row in rows:
        if "error" in row:
            errors.append(row)
            continue
        counts.update(row["counts"])
        malformed.update(row["malformed"])
        gap_buckets.update(row["frameGaps"]["buckets"])
        gap_max = max(gap_max, row["frameGaps"]["maxSeconds"] or 0.0)
        without_team_wins += row["teamWinsEvents"] == 0
    return {
        "schemaVersion": SCHEMA_VERSION,
        "scope": "replayStateCorpusAudit",
        "summary": {
            "discovered": len(paths),
            "decoded": len(rows) - len(errors),
            "failed": len(errors),
            "counts": dict(sorted(counts.items())),
            "malformed": dict(sorted(malformed.items())),
            "frameGapBuckets": dict(sorted(gap_buckets.items())),
            "maximumFrameGapSeconds": gap_max,
            "withoutTeamWins": without_team_wins,
        },
        "errors": errors,
        "replays": rows,
    }


def _write_json(path: pathlib.Path, document: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(document, separators=(",", ":")) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    export = subparsers.add_parser("export", help="export one exact checkpoint stream")
    export.add_argument("replay", type=pathlib.Path)
    export.add_argument("--json", required=True, type=pathlib.Path)
    export.add_argument(
        "--map-archive",
        action="append",
        default=[],
        type=pathlib.Path,
        help="explicitly ordered SDF archive to use for matching map art/bounds",
    )

    audit = subparsers.add_parser("audit", help="audit one or more replay roots")
    audit.add_argument("roots", nargs="+", type=pathlib.Path)
    audit.add_argument("--json", required=True, type=pathlib.Path)
    audit.add_argument("--jobs", type=int, default=1)
    audit.add_argument("--summary-only", action="store_true")

    args = parser.parse_args()
    if args.command == "export":
        document = decode_replay(args.replay, map_archives=args.map_archive)
        _write_json(args.json, document)
        print(
            f"exported {document['coverage']['counts'].get('unitFrame', 0)} "
            f"frames across {len(document['units'])} unit generations"
        )
        return 0

    paths = _discover(args.roots)
    document = audit_replays(paths, args.jobs)
    if args.summary_only:
        document.pop("replays", None)
    _write_json(args.json, document)
    summary = document["summary"]
    print(
        f"decoded {summary['decoded']}/{summary['discovered']} replays; "
        f"{summary['failed']} failed"
    )
    return 1 if summary["failed"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
