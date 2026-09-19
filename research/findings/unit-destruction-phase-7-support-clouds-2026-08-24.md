# Unit destruction Phase 7: support damage clouds

Date: 2026-08-24

## Decision

Do not attribute a destruction from an active support cloud alone. The replay wire
identifies the cloud definition, position, lifetime, and owning faction, but not the
victim or the particular cloud instance that supplied the fatal health change. The
strict replay-only rule described below recovered zero deaths in the full corpus, so
this phase does not change canonical parser or viewer output.

## Binary and shipped-data evidence

The inspected dedicated server is 32-bit `wic_ds.exe` 1.0.1.1 b35, SHA-256
`c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf`.

- `0x004791b0` maps unit metatypes and `0x00479230` maps unit categories;
  `0x0047a8e0` loads both fields into the runtime unit type.
- The cloud update at `0x004fbd80` filters category and friendly/enemy relation,
  applies the metatype damage multiplier, uses exact 3D center distance, and sends
  lethal health changes through the ordinary fatal path. A null killer becomes the
  `UnitDestroy.aKiller == 512` sentinel.
- The constructor at `0x004fbb30` records expiration and first-damage timing while
  retaining owner/damage-source state internally. Projectile death parasites create
  clouds through `0x00520010`; the server loop at `0x004b2970` updates projectiles
  before clouds, permitting creation and damage in the same tick.
- Replay `CreateCloud` omits victim identity and fatal-source cloud identity. Its
  `aTeam` field is the owning faction, not an individual player.

The shipped definitions contain 122 multiplayer units: 21 AIR, 63 GROUND, and 38
INFANTRY. They contain 79 support-cloud definitions, including 55 damaging clouds
and 24 damaging multiplayer clouds covering napalm, tank busters, gas, and tactical
nuke layers. The decoder now exposes each unit's metatype and category so the audit
can apply the server's actual filters.

## Candidate rule and validation

Audit schema v13 joins an active `CreateCloud` to its exact shipped definition and
requires damaging multiplayer data, a valid faction, lifetime coverage, affected
unit category and friendly/enemy relation, negative effective damage, and a victim
frame inside the cloud. The production-strength subset additionally requires a
fresh cloud and victim frame on the destruction's exact raw tick, a definitely
inside position after coordinate quantization, one-tick damage at least equal to
the victim's maximum health, and no building occupancy.

Command:

```bash
PYTHONPATH=scripts "$HOME/.venvs/wic-analysis/bin/python" \
  scripts/unit_destruction_attribution_audit.py \
  replays/main replays/settings replays/wicgate-documents \
  --jobs 4 --summary-only \
  --json /tmp/unit-destruction-cloud-corpus-v13.json
```

The run analyzed all 2,880 paths without audit failures. Report SHA-256:
`a53e11978da1f50ac83bf1248f18f956efb7dc959bd0e6dff8bc18a3ae11fd48`.

| Population | Deaths | Active damaging cloud | Strict sufficient |
| --- | ---: | ---: | ---: |
| Known active-killer controls | 446,586 | 71,063 | 0 |
| Existing exact-TA controls | 15,604 | 3,236 | 0 |
| Remaining unknown | 170,688 | 78,014 | 0 |

The 78,014 unknown overlaps cannot be promoted: the same broad test also overlaps
71,063 deaths whose ordinary killer is already known. Focused Quarry examples show
fresh cloud/death ticks but victim-frame ages of roughly 0.03 to 0.41 seconds. That
cadence prevents proof of exact fatal-time geometry; one representative tank-buster
overlap is spatially close but has only -300 damage against 750 maximum health.

## Final fatal-path inventory

A final Ghidra cross-reference pass bounded the dedicated server's central unit
fatal routine rather than searching replay messages by name alone. Ghidra currently
records exactly three direct code callers of `wic_ds.exe:0x004ccef0`:

| Direct caller | Role established so far |
| --- | --- |
| `0x004ce310` | Normal health-damage path; carries killer unit, direction, internal damage-source pointer, and support index into the fatal call. |
| `0x004ce880` | Forced death helper; supplies killer sentinel `0x200`, null damage source, and a synthetic direction. |
| `0x004ce8a0` | Synthetic-direction inherited death used by the proven building-resident and container-child paths. |

The fatal routine stores the killer unit at `+0x234` and the internal damage-source
pointer at `+0x24c` before broadcasting destruction. The replay `UnitDestroy` writer
does not serialize `+0x24c`; once the killer is `0x200`, that source distinction is
not recoverable from that message alone.

The normal health-damage routine has three direct callers: `0x004ce830`,
`0x004d26b0`, and the unit damage virtual at `0x004d2bd0`. The inherited-death
helper's callers are already accounted for by resident manager `0x00518d30` and
container destruction `0x005108d0`.

The remaining unclassified binary surface is the six direct callers of forced-death
helper `0x004ce880`: `0x004ddf20`, `0x004d37c0`, `0x00506bd0`, `0x00536670`,
`0x004a6ac0`, and `0x00513260`. Their gameplay meanings and any companion replay
messages were not established before this investigation stopped. They are a bounded
future task, not evidence for a current classification.

The player-disband branch remains separately bounded. Shared blink scheduler
`0x004ca3e0` is called by the proven disband request at `0x004a8ac0`, recursively,
and by `0x004d3150` and `0x004d5810`; those latter paths originate through
`0x004b1490`/`0x004aefd0` and `0x004b7d20`, respectively. Because the replay-visible
state transition omits the initiating caller/reason, caller identity must be joined
to a distinct serialized companion event before it can distinguish deletion.

## Remaining avenues

1. Decompile and classify all six `0x004ce880` callers, then search the complete
   169-message writer inventory for a unique replay-visible companion emitted on
   each path. Positive controls and counterexample screening are mandatory.
2. Trace the non-disband callers of `0x004ca3e0` through their request/callback
   origins and compare their exact emitted state sequences with disband. Do not use
   blink timing unless a distinct serialized reason survives those controls.
3. Search for another serialized damage-source or kill-credit bridge; the server's
   retained pointer is not present in `CreateCloud` or `UnitDestroy`.
4. Investigate whether a rigorously proven movement bound can make any stale victim
   frame conclusive. This must cover transports, teleports, modifiers, and coordinate
   quantization before it can become more than candidate evidence.
5. Runtime/server instrumentation can prove future damage paths, but cannot recover
   missing source identity from historical replay files.
