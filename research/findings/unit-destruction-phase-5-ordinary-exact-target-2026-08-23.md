# Ordinary exact-target projectile audit

## Scope

Phase 5 tests whether an ordinary homing projectile that names the exact active
victim lifecycle can attribute a sentinel-512 complete-unit destruction without
the independent `UnitDestroy.aKiller` bridge required by parser schema v15. It
tests both exact player attribution and the narrower firing-team attribution.

This is audit schema v8 only. It does not change canonical parser or viewer
output.

The full ignored report is
`/tmp/unit-destruction-phase5-team-corpus-v8.json`, SHA-256
`06fd8c20330fb1317f6da8e3ae85c3498b492ff268167fd735aa35aebbe16be1`.
It covers 2,880 deduplicated replay paths with zero failures.

## Candidate rule

For each complete-unit destruction, the audit considers ordinary
`ProjectileHomingUnitCreate` records that:

1. precede the destruction in both event order and replay time;
2. occur no more than 0.5 seconds earlier;
3. serialize the destroyed unit ID as `aTarget`;
4. resolve that target to the same active unit-creation offset as the victim; and
5. resolve the firing unit to an active player and team at projectile time.

The player candidate requires exactly one firing actor. The weaker team candidate
allows multiple actors only when all resolve to one team. Unlike schema v15, this
experiment deliberately has no `aKiller` unit with which to prove which projectile
was lethal.

## Corpus result

| Metric | Count |
|---|---:|
| Complete-unit known-killer controls | 447,406 |
| Controls with exactly one firing actor | 19,364 |
| Actor agrees with serialized killer player | 16,710 |
| Actor contradicts serialized killer player | 2,654 |
| Controls with one unanimous firing team | 19,708 |
| Team agrees with serialized killer team | 19,704 |
| Team contradicts serialized killer team | 4 |
| Remaining unknown complete-unit deaths | 170,954 |
| Unknowns with exactly one firing actor | 173 |
| Unknowns with one unanimous firing team | 175 |

The player candidate is wrong in 13.70% of the controls where it returns one
actor. Most contradictions are assisting fire from a teammate rather than the
lethal shot, so reducing player identity to team substantially improves the
control result but does not make it exact.

A team-level counterexample occurs in
`replays/main/WicTracker/downloads/1083__demo27.wicdemo` at raw time
`280.388550s`. Unit 295, owned by player 6 on team 2, is destroyed with killer
unit 295 and killer player 6 (a serialized self-kill). A homing projectile from
player 5 on team 3 targets the exact same unit lifecycle within the candidate
window. Dropping the killer-unit bridge would falsely assign the destruction to
team 3.

## Conclusion

An exact homing target proves that a unit was being attacked, not that this
particular projectile caused its destruction. The independent killer-unit equality
in schema v15 is essential. Neither unique player nor unanimous team is safe after
removing it, and the candidate would cover only 0.10% of the remaining unknowns at
team level even if its four demonstrated contradictions were ignored.

No parser or viewer attribution is justified. Sentinel-512 deaths in this subset
remain unknown. A future ordinary-fire phase needs a distinct replay-visible
lethal-damage or impact bridge; shrinking the timing window alone cannot turn
targeting into proof of causation.
