# Temporal player identity sessions — 2026-08-20

## Finding

A replay player ID is a reusable numeric slot, not a match-long human identity.
Static roster resolution remains reliable for slots that never change hands, but it
can misattribute later events after an occupant leaves and another player enters the
same slot.

`PlayerEntersGame` carries the exact slot and a bounded UTF-16 occupant name in a
stable type-5 field whose shipped identifier string is still unknown.
`PlayerLeavesGame` carries the slot that becomes vacant. Replaying those lifecycle
messages in byte order therefore supplies an evidence-backed identity interval
without guessing from score, role, team, or timing proximity.

## Trigger replay

Evidence file:

- path: `replays/main/WicTracker/downloads/744__demo08.wicdemo`
- SHA-256: `7e0b67507afdf001b9df502d83784edc8f0501859ad94c4e599bc3c6b918202f`

Slot 1 first enters as `[-HH-]JonnySky`, leaves, and later enters as
`[WHO]LtDan73`. The old static participant directory continued to label slot 1 as
JonnySky, incorrectly naming the slot's tactical-aid markers at timeline seconds
402.673 and 1069.512. The same replay contains three additional replacements:

| Slot | Earlier occupant | Later occupant |
|---:|---|---|
| 8 | `龍^blahdy` | `Todesengel22` |
| 11 | `[-HH-]gizey` | `bTd^westurkey` |
| 12 | `Diceman` | `[RD]Biotoxin` |

## Corpus result

The read-only structural audit scanned 2,880 unique linked replay paths with no
parse failures:

| Measure | Count |
|---|---:|
| `PlayerEntersGame` envelopes | 33,051 |
| Entry envelopes without a decodable bounded name | 2 |
| `PlayerLeavesGame` envelopes | 8,023 |
| Malformed leave envelopes | 0 |
| Replays with at least one slot carrying different names | 983 |
| Reused slot instances across those replays | 2,525 |

This is common corpus behavior rather than a one-replay edge case. The two damaged
entry-name records still establish a slot transition but must open a name-null
session.

## Implemented boundary

Timeline schema v11 adds `participantSessions` while retaining `participants` as a
compatibility fallback:

- the static participant directory is unchanged for stable slots and older output;
- a valid entry opens an inclusive session with the serialized name;
- a different or undecodable entry closes the former session and opens a new one;
- a leave closes the active session at the end of its envelope;
- session ends are exclusive;
- a post-leave gap is unknown and never inherits the former static name; and
- viewer attribution resolves every event at its event time, preferring an explicit
  chat-level name when present.

No unknown identity is invented. The schema records `staticMetadata` or
`playerEnteredGame` as the identity source, and keeps the numeric player ID as the
serialized fact.

## Reproduction

```bash
PYTHONPATH=scripts "$HOME/.venvs/wic-analysis/bin/python" \
  scripts/player_identity_session_audit.py \
  replays/main replays/settings replays/wicgate-documents --jobs 8 \
  --json findings/player-identity-session-corpus-audit.json
```

The bulk JSON contains private corpus paths and remains ignored. This reviewed
finding is the tracked summary.
