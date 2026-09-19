"""Historical role-extension probe; requires the v40 audit containing role evidence."""

import argparse
import bisect
import collections
import hashlib
import json
import struct
import sys
import zlib
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
inv = json.load(open("local/generated/bintag-message-inventory.json"))
messages = {int(v["hash"], 16): v["name"] for v in inv["messages"]}
fields = {zlib.adler32(f.encode()): f for v in inv["messages"] for f in v["fields"]}
role_names = {
    0x0E0002CB: "infantry",
    0x1ABF03DF: "support",
    0x10E00313: "armor",
    0x0AE8026E: "air",
}
paths = {r["path"]: r for r in json.load(args.audit.open())["changes"]}
selected = []
for suffix, ids in [
    ("333__demo17.wicdemo", [3, 15, 6]),
    ("/Test Replays/demo66.wicdemo", [4]),
    ("/Test Replays/demo33.wicdemo", [1]),
    ("358__demo85.wicdemo", [4, 6, 15]),
    ("/Test Replays/demo69.wicdemo", [12]),
]:
    path = next(p for p in paths if p.endswith(suffix))
    selected.append((path, ids))
results = []
for path, ids in selected:
    p = m.WICReplayParserV4(path)
    d = p.get_full_decompressed_data()
    c = p._find_chain_anchor(d)
    chain = []
    max_time = 0
    primary = True
    while e := p._read_envelope(d, c):
        end, time = e
        if time < max_time - m.RECORDING_RESET_DROP_SECONDS:
            primary = False
        max_time = max(time, max_time)
        pos = c + m._ENVELOPE_MESSAGE_OFFSET
        chain.append((c, end, pos, time, primary))
        c = end
    starts = [e[0] for e in chain]
    allroles = []
    for offset in p._find_all(d, m._HASH_AROLE_ID):
        value = p._read_bintag_u32(d, offset)
        if value is None:
            continue
        i = bisect.bisect_right(starts, offset) - 1
        e = chain[i] if i >= 0 and offset < chain[i][1] else None
        prior = offset - 17
        slot = (
            p._read_bintag_u32(d, prior)
            if d[prior : prior + 4] in [m._HASH_ASLOT, m._HASH_APOS]
            else None
        )
        row = {
            "offset": offset,
            "slot": slot,
            "roleId": hex(value),
            "role": role_names.get(value),
            "message": messages.get(
                struct.unpack_from("<I", d, e[2])[0],
                hex(struct.unpack_from("<I", d, e[2])[0]),
            )
            if e
            else None,
            "time": e[3] if e else None,
            "primary": e[4] if e else False,
        }
        if e and row["message"] == "SetScoreAtGameEnd":
            row["roleScores"] = {
                str(k): p._read_bintag_u32(d, o)
                for k, key in enumerate(
                    [
                        m._HASH_SCORE_ROLE0,
                        m._HASH_SCORE_ROLE1,
                        m._HASH_SCORE_ROLE2,
                        m._HASH_SCORE_ROLE3,
                    ]
                )
                for o in p._find_all(d[e[2] : e[1]], key)
                for o in [o + e[2]]
            }
        allroles.append(row)
    result = {
        "path": path,
        "sha256": hashlib.sha256(Path(path).read_bytes()).hexdigest(),
        "bytes": len(d),
        "chainEnd": c,
        "roleRecords": len(allroles),
        "roleMessages": dict(collections.Counter(r["message"] for r in allroles)),
        "targets": {str(id): [r for r in allroles if r["slot"] == id] for id in ids},
        "unassignedRoleFields": [r for r in allroles if r["slot"] is None],
    }
    # Known role values elsewhere: identify the immediately preceding field hash.
    other = collections.Counter()
    for val in role_names:
        for off in p._find_all(d, struct.pack("<I", val)):
            if (
                off >= 13
                and struct.unpack_from("<I", d, off - 9)[0] == 17
                and struct.unpack_from("<I", d, off - 4)[0] == 17
            ):
                key = struct.unpack_from("<I", d, off - 13)[0]
                other[fields.get(key, hex(key))] += 1
    result["allKnownRoleValueFields"] = dict(other)
    results.append(result)
    print(json.dumps(result, ensure_ascii=False), flush=True)
Path("local/generated/missing-role-timeline-probe-2026-09-07.json").write_text(
    json.dumps(results, ensure_ascii=False, indent=2)
)
print(
    "Inventory role-bearing messages:",
    [
        (v["name"], v["fields"])
        for v in inv["messages"]
        if any("role" in f.lower() for f in v["fields"])
    ],
)
