"""Independently inspect candidate slots using the Python reference decoder."""

import argparse
import collections
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "parser"))
import wic_replay_parser as m  # noqa: E402


def inspect(case, raw):
    path = case["input"]["path"]
    assert (
        hashlib.sha256(Path(path).read_bytes()).hexdigest() == case["input"]["sha256"]
    )
    slots = {c["slot"] for c in case.get("candidates", [])}
    slots.update(c["slot"] for c in case.get("scoreDifferences", []))
    p = m.WICReplayParserV4(path)
    d = p.get_full_decompressed_data()
    cursor = p._find_chain_anchor(d)
    events = []
    counts = collections.Counter()
    last_scores = {}
    max_time = 0
    while envelope := p._read_envelope(d, cursor):
        end, time = envelope
        if time < max_time - m.RECORDING_RESET_DROP_SECONDS:
            break
        max_time = max(max_time, time)
        pos = cursor + m._ENVELOPE_MESSAGE_OFFSET
        tag = d[pos : pos + 4]
        if tag == m._HASH_TEAM_WINS:
            break

        def field(off, key):
            at = pos + off
            return (
                p._read_bintag_u32(d, at)
                if at + 17 <= end and d[at : at + 4] == key
                else None
            )

        if tag == bytes([41, 3, 220, 13]):
            slot = field(4, m._HASH_APOS)
            score = field(21, m._HASH_SCORE)
            if (
                slot in slots
                and score is not None
                and d[pos + 12] == 1
                and d[pos + 29] == 0
            ):
                last_scores[slot] = [
                    cursor,
                    time,
                    struct.unpack_from("<i", d, pos + 34)[0],
                ]
        elif tag == m._HASH_UNIT_CREATE:
            slot = field(21, m._HASH_APLAYER)
            if slot in slots:
                counts[
                    (slot, "pregameUnits" if cursor < raw["start"] else "gameplayUnits")
                ] += 1
        else:
            slot = field(4, m._HASH_ASLOT)
            if slot in slots:
                kind = None
                detail = None
                if tag == m._HASH_PLAYER_ENTERS_GAME:
                    kind = "entry"
                    at = pos + 38
                    if at + 13 <= end and d[at : at + 4] == m._HASH_PLAYER_ENTRY_NAME:
                        size = struct.unpack_from("<I", d, at + 4)[0]
                        if at + size <= end:
                            detail = p._read_bintag_utf16_string(
                                d,
                                at,
                                m._HASH_PLAYER_ENTRY_NAME,
                                trim_trailing_nbsp=True,
                            )
                elif tag == m._HASH_PLAYER_LEAVES_GAME:
                    kind = "leave"
                elif tag == m._HASH_PLAYER_JOINED_TEAM:
                    kind, detail = "team", field(21, m._HASH_ATEAM)
                elif tag == m._HASH_SPECTATOR_JOINED_TEAM:
                    kind, detail = "spectator", field(21, m._HASH_ATEAM)
                elif tag == m._HASH_PLAYER_SET_ROLE:
                    kind, detail = "role", field(21, m._HASH_AROLE_ID)
                if kind:
                    events.append(
                        dict(
                            slot=slot,
                            kind=kind,
                            value=detail,
                            offset=cursor,
                            time=time,
                            gameplay=cursor >= raw["start"],
                        )
                    )
        cursor = end
    return dict(
        sha256=case["input"]["sha256"],
        path=path,
        events=events,
        counts={
            str(slot): {
                kind: count for (sid, kind), count in counts.items() if sid == slot
            }
            for slot in slots
        },
        lastScores=last_scores,
    )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("comparison", type=Path)
    cli.add_argument("extraction", type=Path)
    cli.add_argument("output", type=Path)
    args = cli.parse_args()
    cases = json.loads(args.comparison.read_text())["cases"]
    raw = {
        r["input"]["sha256"]: r["data"]
        for r in map(json.loads, args.extraction.open())
        if "data" in r
    }
    selected = [c for c in cases if c.get("candidates") or c.get("scoreDifferences")]
    results = []
    for i, c in enumerate(selected, 1):
        r = inspect(c, raw[c["input"]["sha256"]])
        for score in c.get("scoreDifferences", []):
            assert r["lastScores"][score["slot"]] == score["lastLive"], (
                r["path"],
                score,
            )
        results.append(r)
        if i % 20 == 0:
            print("Verified", i, flush=True)
    args.output.write_text(json.dumps(results, ensure_ascii=False, indent=2))
    print("Verified", len(results), "distinct candidate/score replays", flush=True)


if __name__ == "__main__":
    main()
