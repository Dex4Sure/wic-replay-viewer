# Tactical Aid taunt, health, and destruction sequence

Date: 2026-09-12. Status: **candidate attribution method; not a production rule**.

## Result

In a selected sample of 100 SHA-256-distinct replays, every one of 4,330
`SendTATaunt` records is immediately followed by a physically contiguous
`UnitHealth` record for a unit owned by the named victim, at the identical raw
recording timestamp. Requiring zero health and a subsequent same-timestamp
`UnitDestroy` for that exact active unit generation selects **1,808 distinct
currently unknown complete player-unit losses**: 1,099 nuclear, 93 carpet-bombing,
and 616 other Tactical Aid candidates.

Another 411 selected deaths already have canonical Tactical Aid causes. All agree
on the aid and faction after the existing, definition-backed Heavy Air Support
child-to-parent mapping. There are no duplicate candidate deaths, conflicting
taunt assignments, or observed ordinary-unit cause conflicts in this sample.
These controls validate an observed pattern, not a universal accuracy estimate.

This is a narrower lead than matching every nearby death to an explosion. It
potentially identifies the unit whose damage crossed the taunt threshold, not
every casualty of that strike. Intervening death callbacks and the full corpus
still need validation before changing attribution. Parser and viewer behavior,
schemas, caches, and product versions are unchanged.

## Binary identity and call sequence

Both inspected executables are stock `1.0.1.1 (b35)`, 32-bit little-endian x86
Windows PE images, with preferred image base `0x00400000`. Addresses below are
static virtual addresses; no running game was attached or patched.

| Evidence | SHA-256 |
| --- | --- |
| `wic_ds.exe` | `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` |
| `wic.exe` | `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` |
| Server `wic_ds.sdf` | `fd6bbe78096f1f5af77cf66cb08423107b54f8cafb25112ebce7a650bd7086b3` |

The dedicated-server damage routine at `0x004ce310` subtracts applied damage from
the unit's health. For eligible support damage it calls `0x004f66a0`, which
accumulates score per affected player, checks `myNumScoreToTriggerTaunt`, and
broadcasts `SendTATaunt` through `0x004b9540` when the threshold is crossed.
The taunt identifies the actor slot, victim slot, support-manager index, and
upgrade level. It does not contain a unit ID or a kill flag.

After that call, `0x004ce310` invokes fatal processing at `0x004ccef0` if health
has reached zero, then broadcasts the damaged unit's current health through
`0x004bb550`. The fatal routine stores death state, killer, and source context;
the replay's later terminal record is checked separately. The inspected damage
routine therefore supplies a concrete reason to investigate the next health
record rather than selecting any death sharing the taunt's timestamp.

This is not yet an exhaustive ordering proof. Calls between the taunt and health
broadcast include fatal processing, the `0x004cc3e0` zero-health path, virtual
component notifications from `0x004c9d10`, and a game callback. Their possible
nested damage/health output must be bounded before adjacency becomes an exact
causal rule. The existing [death-explosion investigation](unit-destruction-phase-9-death-explosion-blasts-2026-08-24.md)
establishes queued blast damage, but does not by itself exhaust those callbacks.

### Replay serialization and rate limiting

Client `0x00b84920` writes the four unsigned taunt fields `aPlayerFrom`,
`aPlayerTaunted`, `aTAId`, and `aSupportUpgradeLevel`. Client `0x00b84340` writes
`UnitHealth` as unsigned `aUnit`, signed `aHealth`, and signed `aDirection`.
Both select recorder channel 0 through `0x00b80880`. The health receive handler
at `0x0093b6c0` calls its replay serializer before resolving the client unit and
updating its state; this inspected receive path does not first filter by fog or
unit ownership.

The stock writer at `0x009240a0` does support rate limiting, but that must not be
generalized to every message. Its constructor at `0x009230d0` initializes an empty
message-rate tree. Setup at `0x00924570` inserts only `UpdateLOS`,
`CameraPosition`, `CameraOrientation`, and `SetGameModeData_Float`; the four
insertion call sites to `0x00436510` are in that setup routine. `UnitFrame` has
separate per-unit throttling. Neither `UnitHealth` nor `SendTATaunt` is configured
for rate limiting in this inspected stock path. This narrows the general writer
caution in the [message inventory](bintag-message-inventory-2026-08-22.md); it does
not prove completeness for corrupted recordings or modified clients.

## Sample and matching method

The baseline canonical parser is the release CLI built from repository commit
`5879e0471a8bdee1022f9185bfba617df8a1b176`, invoked with `--timeline-json`.
The read-only Python probe uses `wic_bintag` and
`unit_destruction_attribution_audit` from `research/scripts/`.

Selection uses the historical 2,880-path audit at
`local/generated/legacy-wic-re/unit-destruction-attribution-audit.json`, SHA-256
`1e2c18974d4477838ed85c8fb532ad89cee9a99607e55e958ea8f09f8bdb7ca9`:

1. Include `demo141.wicdemo` and the Quarry control below, the top 20 rows ordered
   by nuclear-taunt count, and the top 12 ordered by carpet-taunt count. The saved
   probe specifies tie-breaking. Deduplicate by content hash, yielding 29 inputs.
2. From the remaining audit rows containing nuclear or carpet taunts, select 71
   additional distinct hashes in ascending SHA-256 order.
3. Verify the selected input hashes. This is a TA-enriched sample, not a random
   sample, 100 independent matches, or a new full-corpus audit.

The probe walks raw envelopes in file order, stopping before a backward recording
clock jump greater than one second to exclude the end-summary segment. It tracks
`UnitCreate` generations and closes them on `UnitRemove` or `UnitDestroy`; reused
unit IDs never borrow a previous generation's ownership. Complete player-unit
deaths exclude the audit helper's 27 infantry-soldier component types and owners
outside slots 0–15.

For each valid four-field taunt, require:

1. The **next envelope**, not the next filtered timeline event, starts at
   `taunt.offset + taunt.total`. Its exact field hashes and types must match the
   three-field `UnitHealth` shape, without trailing data.
2. Its active unit owner equals `aPlayerTaunted`, and its raw float32 timestamp
   equals the taunt timestamp exactly. No rounded-time or subsecond tolerance is
   used for this join.
3. For a terminal candidate, health equals zero and the same unit generation has
   a later-offset complete-unit `UnitDestroy` at that identical raw timestamp.
4. Compare against the canonical death using unit ID and its millisecond-rendered
   time, requiring a unique match within 0.001 seconds. That tolerance is only
   for comparison with JSON output, not for establishing the raw causal candidate.
5. Deduplicate by replay SHA-256 and death offset; check conflicting actor,
   victim, support, index, and upgrade assignments.

The probe resolves the taunt index using ordered zero-position `SupportThingUsed`
rows and shipped support names. Unlike the production catalogue reader, this
diagnostic snapshot does not exclude a priced use at the world origin using its
preceding honors deduction. Such catalogue edge cases, malformed input handling,
missing catalogues, and temporal player identities need explicit validation in
any production implementation. Exact raw adjacency does not establish that the
entire earlier replay was free of reader resynchronization.

## Measurements

| Observed stage | Nuclear | Carpet bombing | Other TA | Total |
| --- | ---: | ---: | ---: | ---: |
| Valid taunts / immediate same-owner, same-time health | 1,297 | 263 | 2,770 | 4,330 |
| Positive health; excluded from terminal candidates | 120 | 148 | 1,588 | 1,856 |
| Zero health | 1,177 | 115 | 1,182 | 2,474 |
| Zero health without a qualifying complete player-unit death | 78 | 22 | 155 | 255 |
| Distinct terminal candidates with canonical cause `unknown` | 1,099 | 93 | 616 | 1,808 |
| Distinct terminal candidates already classified `tacticalAid` | 0 | 0 | 411 | 411 |

The 255 excluded zero-health records are outside the complete-unit death join;
they must not be relabeled as survivors or additional complete-unit kills.
No eligible complete-unit candidate failed the same-time terminal ordering check.
No selected terminal had a canonical destruction context. No taunt-to-health
physical gaps, owner mismatches, duplicate deaths, or conflicting assignments
were observed in this sample.

Of the 411 existing TA controls, 101 match the raw support ID directly. The other
310 use Heavy Air Support child projectile IDs while the taunt names the parent
aid. Every pair agrees with the explicit mapping in
[`tactical_aid_projectile_name`](../../parser/rust_parser/src/parser.rs), derived
from shipped definitions. Raw child IDs must remain distinct from parent IDs;
agreement at the parent level is not literal ID equality. These controls test aid
and faction agreement, not independent confirmation of the taunt's attacker slot.

## Quarry evidence packet

Replay: `local/replays/main/old/4v4 MM Quarry 2010 #1 Dexter pr0 Support.wicdemo`.
SHA-256: `c0bab85221538f158e1b077eaae033245f308d38c1d273b23a4c2454ecd1a826`.
Recorder/victim slot 0 is `°Ðexter°`; actor slot 13 is `-=/LK/=-THE_MONTY`.
The following offsets are in the decompressed replay. All four records have raw
recording time **1564.34765625 seconds**, approximately **26:04.348**, not a match
countdown time.

| Offset | Record | Relevant fields |
| --- | --- | --- |
| `0x122acfb` | `SendTATaunt` | actor 13, victim 0, index 53 (`TacticalNuke_USSR`), upgrade 1 |
| `0x122ad54` | `UnitHealth` | unit 100, health 0, direction 0 |
| `0x122ad9c` | `UnitHealth` | unit 140, health 837 |
| `0x122ade4` | `UnitDestroy` | unit 100, killer 512 |

The taunt envelope is 89 bytes, so its end is exactly the start of unit 100's
72-byte health envelope. Unit 100's active generation began at `0x246ab7`.
The baseline parser reports its cause as `unknown`. The intervening health update
for unit 140 illustrates why the terminal check follows the exact unit lifetime
rather than assuming the next record after zero health must be its destruction.

## Terrain, scope, and remaining proof

Blast radius is not a guaranteed kill radius. Server area-damage routine
`0x0051f0b0` calls geometry helper `0x004434b0` at `0x0051f29f` and `0x0051f97d`;
blocking results bypass damage in the inspected branches. The helper reaches
`0x00442b90` and height-map intersection through `0x0045ab50` / `0x00459920`.
Thus terrain can affect damage. This inspection does not reconstruct every crater
or explain each observed survivor. The health-sequence probe needs neither a
guaranteed lethal radius nor interpolation of recorded unit positions.

Before promoting a rule, finish the intervening-callback audit and validate the
complete corpus, including nonlethal taunts, same-victim simultaneous damage,
container/building deaths, infantry components, reused IDs, player-slot changes,
catalogue edge cases, and malformed recordings. Retain unknowns whenever the
chain is absent, ambiguous, or outside the proven conditions. Do not spread a
taunt's actor to nearby deaths, an entire nuclear cloud, or all projectiles of a
strike. The earlier [same-timestamp taunt screen](../notes/unit-destruction-attribution-verification.md#exact-tactical-aid-damage-notification)
and [area-effect rejection](unit-destruction-phase-2-area-effects-2026-08-23.md)
remain valid; this report adds a health-record constraint and a damage-code lead.

## Local reproduction artifacts

The completed diagnostic snapshot is preserved, untracked, under
`local/generated/taunt-health-destruction-sequence-2026-09-12/`:

| Artifact | SHA-256 or purpose |
| --- | --- |
| `probe.py` | `4ea13e1620b41d1b984126f0599add73c31a60027bf13d15f32ef256399d1df4` |
| `results.txt` | `6f1cd5e30fa0cdd98a359c1aa93b85edfc9d54c984974df10bc7201dd2ed2bb8` |
| `sample-manifest.json` | `a55d22b8aa934222e8471e0a37741e69f253f0e8290a76eb0d150ae1e371b89c` |
| `binary-inspection.json` | Saved read-only decompiler responses supporting this investigation |
| `binary-ordering-details.json` | Writer constructor and server taunt/health broadcast decompilations |

The manifest lists all 100 paths and verified input hashes. These are local
research artifacts, not a maintained portable CLI or files supplied by a fresh
clone. With the private inputs, historical audit, and baseline release parser
available, rerun from the repository root:

```bash
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH=research/scripts \
  "$HOME/.venvs/wic-analysis/bin/python" \
  local/generated/taunt-health-destruction-sequence-2026-09-12/probe.py
```

This prints diagnostics and aggregates without changing source replays, the
canonical parser, or viewer data. `RAW_ID_MISMATCH_SUMMARY.namesDisagree` must be
empty; the 310 raw-ID differences are the documented Heavy Air Support controls.
