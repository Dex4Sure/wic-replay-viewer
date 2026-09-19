"""Historical role-extension probe; requires the v40 audit containing role evidence."""

import unicodedata
import argparse
import bisect
import collections
import json
import sys
import struct
from pathlib import Path

sys.path.insert(0, "parser")
import wic_replay_parser as m  # noqa: E402

cli = argparse.ArgumentParser(description=__doc__)
cli.add_argument(
    "audit",
    type=Path,
    help="Historical last-role-audit.json with lastRecordedRole fields",
)
args = cli.parse_args()


def entry_name(d, pos, end):
    field = pos + 38
    if field + 15 > end or d[field : field + 4] != m._HASH_PLAYER_ENTRY_NAME:
        return None
    size = struct.unpack_from("<I", d, field + 4)[0]
    if (
        size < 15
        or d[field + 8] != 5
        or struct.unpack_from("<I", d, field + 9)[0] != size
        or field + size > end
    ):
        return None
    payload = d[field + 13 : field + size]
    if len(payload) % 2 or not payload.endswith(b"\0\0"):
        return None
    try:
        value = payload[:-2].decode("utf-16le").rstrip("\xa0")
    except UnicodeDecodeError:
        return None
    return (
        value
        if value and not any(unicodedata.category(c) == "Cc" for c in value)
        else None
    )


j = json.load(open("local/generated/missing-role-entry-corpus-probe-2026-09-07.json"))
audit = {r["path"]: r for r in json.load(args.audit.open())["changes"]}
inv = json.load(open("local/generated/bintag-message-inventory.json"))
msgs = {int(v["hash"], 16): v["name"] for v in inv["messages"]}
out = []
selected = {r["path"] for r in j["knownRoleRecords"]} | {
    r["path"] for r in j["knownEntryValues"]
}
for r in j["replays"]:
    if r["path"] not in selected:
        continue
    p = m.WICReplayParserV4(r["path"])
    d = p.get_full_decompressed_data()
    c = p._find_chain_anchor(d)
    chain = []
    starts = []
    entries = []
    result = None
    max_time = 0
    primary = True
    while e := p._read_envelope(d, c):
        end, t = e
        pos = c + m._ENVELOPE_MESSAGE_OFFSET
        tag = d[pos : pos + 4]
        if t < max_time - m.RECORDING_RESET_DROP_SECONDS:
            primary = False
        max_time = max(max_time, t)
        starts.append(c)
        chain.append((c, end, pos, t, primary))
        if tag == m._HASH_TEAM_WINS and result is None:
            result = (pos, t)
        if tag in [m._HASH_PLAYER_ENTERS_GAME, m._HASH_PLAYER_LEAVES_GAME]:
            slot = p._read_bintag_u32(d, pos + 4)
            if slot in r["targets"]:
                entries.append(
                    dict(
                        slot=slot,
                        offset=pos,
                        time=t,
                        kind="enter" if tag == m._HASH_PLAYER_ENTERS_GAME else "leave",
                        name=entry_name(d, pos, end)
                        if tag == m._HASH_PLAYER_ENTERS_GAME
                        else None,
                    )
                )
        c = end
    roles = []
    for role in r["roleRecords"]:
        i = bisect.bisect_right(starts, role["offset"]) - 1
        e = chain[i] if i >= 0 and role["offset"] < chain[i][1] else None
        roles.append(
            dict(
                **role,
                message=msgs.get(struct.unpack_from("<I", d, e[2])[0]) if e else None,
                time=e[3] if e else None,
                primary=e[4] if e else False,
            )
        )
    row = dict(
        path=r["path"],
        sha256=r["sha256"],
        players=audit[r["path"]]["players"],
        evidence=audit[r["path"]]["evidence"],
        entries=entries,
        roles=roles,
        result=result,
        initialEntries=r["entries"],
    )
    out.append(row)
Path("local/generated/missing-role-late-records-2026-09-07.json").write_text(
    json.dumps(out, ensure_ascii=False, indent=2)
)
counts = collections.Counter()
safe = []
for r in out:
    for e in r["roles"]:
        counts[e["message"]] += 1
        slot = e["slot"]
        ev = [q for q in r["entries"] if q["slot"] == slot]
        own = next(q for q in r["evidence"] if q["playerId"] == slot)
        left = own["scoreBeforeLeave"]["leftAtSeconds"]
        later = [q for q in ev if q["kind"] == "enter" and q["time"] >= left]
        counts["with later entry" if later else "no later entry"] += 1
        if not later:
            safe.append(dict(path=r["path"], role=e, entries=ev))
print("Counts", dict(counts))
print("No later entry", json.dumps(safe, ensure_ascii=False))
for r in out:
    if "Wilda" in r["path"]:
        print("Initial role candidate", json.dumps(r, ensure_ascii=False))
