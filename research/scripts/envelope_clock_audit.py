#!/usr/bin/env python3
"""Corpus scan: the Event envelope clock against the game-mode countdown.

Evidence for timeline schema v13, which times every timeline record from its own
`Event` envelope instead of interpolating between countdown samples. Read-only;
it never writes to `local/replays/`.

Per replay it reports the recording length from the envelope chain, where the
end-of-match summary pass restarts the clock, the countdown's first and last
samples, and the ratio of envelope seconds to countdown seconds.

Usage:

    python research/scripts/envelope_clock_audit.py local/replays/main/*.wicdemo
"""

import json
import pathlib
import sys
from concurrent.futures import ProcessPoolExecutor

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import wic_bintag as bt  # noqa: E402

M_MODEFLOAT = int.from_bytes(bytes([0xFB, 0x07, 0x91, 0x56]), "little")
M_TEAMWINS = int.from_bytes(bytes([0x29, 0x03, 0xB8, 0x0D]), "little")
F_DATATYPE = int.from_bytes(bytes([0x7E, 0x03, 0xD6, 0x10]), "little")
F_AFLOAT = int.from_bytes(bytes([0x58, 0x02, 0xDD, 0x07]), "little")
RESET_DROP = 5.0


def probe(path):
    try:
        data = bt.decompress(path)
    except Exception as e:
        return {"file": pathlib.Path(path).name, "error": f"{type(e).__name__}: {e}"}
    prev = None
    n = 0
    resets = []
    primary_max = 0.0
    in_primary = True
    small_backsteps = 0
    clock = []  # (t, remaining) primary chain only
    teamwins_t = None
    for env in bt.walk(data):
        n += 1
        t = env.time
        if prev is not None and t < prev:
            if prev - t > RESET_DROP:
                resets.append(n)
                in_primary = False
            else:
                small_backsteps += 1
        prev = t
        if in_primary:
            primary_max = max(primary_max, t)
            if env.message == M_MODEFLOAT:
                flds, _ = bt.fields(data, env)
                dt = val = None
                for f in flds:
                    if f.hash == F_DATATYPE:
                        dt = f.u32
                    elif f.hash == F_AFLOAT:
                        val = f.f32
                if dt == 1 and val is not None and val != 0.0 and abs(val) < 100000:
                    clock.append((t, val))
            elif env.message == M_TEAMWINS and teamwins_t is None:
                teamwins_t = t

    out = dict(
        file=pathlib.Path(path).name,
        dir=pathlib.Path(path).parent.name,
        envelopes=n,
        recording_seconds=round(primary_max, 3),
        resets=len(resets),
        small_backsteps=small_backsteps,
        clock_samples=len(clock),
        teamwins_t=round(teamwins_t, 3) if teamwins_t is not None else None,
    )
    if clock:
        t0, r0 = clock[0]
        t1, r1 = clock[-1]
        out.update(
            countdown_start_t=round(t0, 3),
            countdown_start_remaining=round(r0, 3),
            countdown_end_t=round(t1, 3),
            countdown_end_remaining=round(r1, 3),
            countdown_span=round(r0 - r1, 3),
            pre_countdown_seconds=round(t0, 3),
            post_countdown_seconds=round(primary_max - t1, 3),
        )
        if (r0 - r1) > 60.0:
            out["rate"] = round((t1 - t0) / (r0 - r1), 5)
    return out


if __name__ == "__main__":
    paths = sys.argv[1:]
    with ProcessPoolExecutor() as ex:
        res = list(ex.map(probe, paths))
    json.dump(res, sys.stdout, indent=1)
