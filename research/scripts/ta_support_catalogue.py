#!/usr/bin/env python3
"""Recover replay tactical-aid names from the shipped support localization.

``maps/supportweapons.loc`` keys contain the internal support definition name and
the English ``myGuiName`` value. The internal names are authoritative candidates
because their Adler-32 hashes can be compared directly with the 200 support IDs
enumerated by replay catalogues. A mapping is emitted only when the name/hash match
is unique and every replay catalogue ID is covered.

The localization text is read directly from an immutable SDF archive; this script
writes only the derived JSON report requested with ``--json``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import zlib
from dataclasses import dataclass, field

from wic_sdf import SdfArchive, SdfError

DEFAULT_ARCHIVE = "local/binaries/server/wic_ds.sdf"
DEFAULT_ENTRY = "maps/supportweapons.loc"
DEFAULT_REPLAY_AUDIT = "local/generated/ta-usage-audit.json"

DEFINITION_KEY = re.compile(
    r"^SupportWeaponDatabase[.]([^.]+)[.]([A-Za-z_][A-Za-z0-9_]*)[.](.+)$"
)
KNOWN_NUCLEAR_NAMES = {
    0x2E8F05C0: "TacticalNuke_US",
    0x3B070665: "TacticalNuke_USSR",
    0x3AB4064A: "TacticalNuke_NATO",
    0x7ADC097E: "TacticalNuke_NATO_British",
}


@dataclass
class LocalizedSupport:
    section: str
    internal_name: str
    gui_names: set[str] = field(default_factory=set)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def parse_support_localization(data: bytes) -> dict[str, LocalizedSupport]:
    """Return one definition record per internal name."""
    try:
        text = data.decode("utf-8-sig")
    except UnicodeDecodeError as error:
        raise ValueError("support localization is not UTF-8") from error

    supports: dict[str, LocalizedSupport] = {}
    for line_number, line in enumerate(text.splitlines(), start=1):
        if "\t" not in line:
            continue
        key, value = line.split("\t", 1)
        match = DEFINITION_KEY.fullmatch(key)
        if match is None:
            continue
        section, internal_name, field = match.groups()
        support = supports.setdefault(
            internal_name,
            LocalizedSupport(section=section, internal_name=internal_name),
        )
        if support.section != section:
            raise ValueError(
                f"definition {internal_name!r} occurs in multiple sections: "
                f"{support.section!r} and {section!r} (line {line_number})"
            )
        if field == "myGuiName":
            support.gui_names.add(value)
    return supports


def presentation_class(support: LocalizedSupport) -> str:
    """Classify definition provenance without claiming purchase semantics."""
    if "CHILD_" in support.internal_name:
        return "childEffect"
    if support.section in {"US", "NATO", "USSR"}:
        return "topLevelFactionAid"
    if support.section == "SpecialAbilities":
        return "specialAbility"
    if support.section == "SinglePlayer":
        return "scriptedSinglePlayer"
    if support.section == "Cinematics":
        return "cinematicEffect"
    return "other"


def build_report(
    archive_path: pathlib.Path,
    entry_path: str,
    replay_audit_path: pathlib.Path,
    marker_audit_path: pathlib.Path | None = None,
) -> dict:
    archive = SdfArchive.open(archive_path)
    matches = [entry for entry in archive.entries if entry.path == entry_path]
    if len(matches) != 1:
        raise ValueError(
            f"expected exactly one {entry_path!r} entry, found {len(matches)}"
        )
    localization = archive.read_entry(matches[0])
    supports = parse_support_localization(localization)

    replay_audit = json.loads(replay_audit_path.read_text())
    summary = replay_audit.get("summary", {})
    catalogue_ids = {int(value, 16) for value in summary.get("catalogue_ids", {})}
    if not catalogue_ids:
        raise ValueError("replay audit contains no catalogue IDs")
    support_costs = summary.get("support_costs", {})
    marker_support_types = {}
    marker_audit_sha256 = None
    if marker_audit_path is not None:
        marker_audit = json.loads(marker_audit_path.read_text())
        marker_support_types = marker_audit.get("summary", {}).get(
            "byDeployedMarkerSupportType", {}
        )
        if not marker_support_types:
            raise ValueError("marker audit contains no deployed marker support types")
        marker_audit_sha256 = sha256_file(marker_audit_path)

    definitions_by_hash: dict[int, list[LocalizedSupport]] = {}
    for support in supports.values():
        support_id = zlib.adler32(support.internal_name.encode("ascii"))
        definitions_by_hash.setdefault(support_id, []).append(support)

    missing = sorted(catalogue_ids - definitions_by_hash.keys())
    collisions = {
        support_id: definitions
        for support_id, definitions in definitions_by_hash.items()
        if support_id in catalogue_ids and len(definitions) != 1
    }
    if missing or collisions:
        missing_text = ", ".join(f"0x{value:08x}" for value in missing) or "none"
        collision_text = (
            ", ".join(f"0x{value:08x}" for value in sorted(collisions)) or "none"
        )
        raise ValueError(
            f"mapping is not exact: missing={missing_text}; collisions={collision_text}"
        )
    unknown_marker_ids = sorted(
        set(marker_support_types) - {f"0x{value:08x}" for value in catalogue_ids}
    )
    if unknown_marker_ids:
        raise ValueError(
            "marker audit IDs are absent from the replay catalogue: "
            + ", ".join(unknown_marker_ids)
        )

    entries = []
    for support_id in sorted(catalogue_ids):
        support = definitions_by_hash[support_id][0]
        cost_evidence = support_costs.get(f"{support_id:08x}")
        marker_evidence = marker_support_types.get(f"0x{support_id:08x}")
        gui_names = sorted(support.gui_names)
        entries.append(
            {
                "supportId": support_id,
                "supportIdHex": f"0x{support_id:08x}",
                "internalName": support.internal_name,
                "section": support.section,
                "presentationClass": presentation_class(support),
                "guiName": gui_names[0] if len(gui_names) == 1 else None,
                "guiNames": gui_names,
                "guiNameStatus": (
                    "exact"
                    if len(gui_names) == 1
                    else "ambiguous"
                    if gui_names
                    else "missing"
                ),
                "activatedByRecorder": cost_evidence is not None,
                "recorderUseCount": (
                    cost_evidence.get("uses", 0) if cost_evidence is not None else 0
                ),
                "observedRecorderCosts": (
                    cost_evidence.get("costs", {}) if cost_evidence is not None else {}
                ),
                "markerDeployments": (
                    marker_evidence.get("deployments", 0)
                    if marker_evidence is not None
                    else 0
                ),
                "markerReplayCount": (
                    marker_evidence.get("replays", 0)
                    if marker_evidence is not None
                    else 0
                ),
                "markerFingerprint": marker_evidence,
                "evidenceClass": "exact",
                "evidenceBasis": "localizedDefinitionNameAdler32EqualsReplayCatalogueId",
            }
        )

    recovered_nuclear = {
        support_id: definitions_by_hash[support_id][0].internal_name
        for support_id in KNOWN_NUCLEAR_NAMES
    }
    if recovered_nuclear != KNOWN_NUCLEAR_NAMES:
        raise ValueError("localization mapping disagrees with executable nuclear names")

    activated_ids = {int(value, 16) for value in support_costs}
    marker_class_totals: dict[str, int] = {}
    for item in entries:
        marker_class_totals[item["presentationClass"]] = (
            marker_class_totals.get(item["presentationClass"], 0)
            + item["markerDeployments"]
        )
    return {
        "schemaVersion": 1,
        "source": {
            "archivePath": str(archive_path),
            "archiveSha256": sha256_file(archive_path),
            "archiveVersion": archive.version,
            "entryPath": entry_path,
            "entrySha256": sha256_bytes(localization),
            "replayAuditPath": str(replay_audit_path),
            "replayAuditSha256": sha256_file(replay_audit_path),
            "markerAuditPath": (
                str(marker_audit_path) if marker_audit_path is not None else None
            ),
            "markerAuditSha256": marker_audit_sha256,
        },
        "summary": {
            "localizedDefinitions": len(supports),
            "replayCatalogueIds": len(catalogue_ids),
            "exactMappings": len(entries),
            "activatedRecorderIds": len(activated_ids),
            "activatedIdsMapped": len(activated_ids & catalogue_ids),
            "hashCollisions": 0,
            "missingCatalogueIds": 0,
            "uniqueGuiNames": sum(len(item["guiNames"]) == 1 for item in entries),
            "ambiguousGuiNames": sum(len(item["guiNames"]) > 1 for item in entries),
            "missingGuiNames": sum(not item["guiNames"] for item in entries),
            "executableNuclearCrossChecks": len(KNOWN_NUCLEAR_NAMES),
            "deployedMarkerIds": len(marker_support_types),
            "markerDeployments": sum(item["markerDeployments"] for item in entries),
            "markerDeploymentsByPresentationClass": dict(
                sorted(marker_class_totals.items())
            ),
        },
        "supports": entries,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", default=DEFAULT_ARCHIVE)
    parser.add_argument("--entry", default=DEFAULT_ENTRY)
    parser.add_argument("--replay-audit", default=DEFAULT_REPLAY_AUDIT)
    parser.add_argument(
        "--marker-audit",
        help="schema-v3 timeline attribution audit with compact marker inventory",
    )
    parser.add_argument("--json", help="write the derived mapping report here")
    args = parser.parse_args()

    try:
        report = build_report(
            pathlib.Path(args.archive),
            args.entry,
            pathlib.Path(args.replay_audit),
            pathlib.Path(args.marker_audit) if args.marker_audit else None,
        )
    except (OSError, SdfError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))

    rendered = json.dumps(report, indent=2) + "\n"
    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(rendered)
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
