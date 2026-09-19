# Tactical Aid marker timers

## Result

Ghidra inspection of the shipped client and dedicated server establishes that
`myMarkerTimeToLive` drives the in-game ground-marker countdown. The shipped
definitions give **35 seconds** for Airborne Infantry, Airdropped Light Tank and
Airdropped Transport, and **15 seconds** for Repair Bridge, in all three factions.
These are display durations, not universal guarantees of unit creation or bridge
repair completion at the exact countdown boundary.

| Definition prefix | Marker lifetime | Initial delay | Activation delay | Recharge |
| --- | ---: | ---: | ---: | ---: |
| `Paratrooper` | 35 | 1 | 16 | 90 |
| `Repair_Jeep` | 35 | 16 | 19.3 US/USSR; 19.6 NATO | 90 |
| `Light_Tank` | 35 | 16 | 19.3 US; 19.4 USSR; 19.6 NATO | 90 |
| `BridgeRepairer` | 15 | 10 | 5 | 30 |

All values are seconds. Each prefix has `_US`, `_USSR`, and `_NATO` definitions.
All twelve have `myMarkerTimeDelay = 0`. Repair Bridge's 30-second recharge and
16-second support-animation lifetime are separate from its 15-second marker.

Aerial Recon (`RadarScan_US`, `RadarScan_USSR`, `RadarScan_NATO`) has a
15-second marker lifetime and 45-second recharge, with zero initial, activation,
and marker delays. Its nested `myLineOfSight` definition independently specifies
`myInitialDelay = 0`, `myTimeToDie = 15`, and `myCircleRadius = 170` in all three
factions. Its countdown therefore represents remaining recon time rather than a
reinforcement arrival delay. The viewer now uses a text label and a 15-second
countdown for these projected rows.

`TacticalNuke` has marker lifetime 16, activation delay 15.7, initial delay 0,
and recharge 450 seconds in US, USSR, NATO and NATO British definitions.
`CarpetBombing` has marker lifetime 15, activation delay 11.7, initial delay 0,
and recharge 90 seconds in all three factions. Both have zero marker delay.
The viewer projects `TacticalNuke` as Nuclear Strike and uses these marker
lifetimes for its text-label countdowns. Expiry does not assert
an exact impact or the completion of the bombing sequence.

## Source identities

Both PE executables report version `1.0.1.1 (b35)`, use 32-bit little-endian x86,
and load at image base `0x00400000`. Addresses below are virtual addresses in
these exact builds. Source files were only read; Ghidra annotations were not changed.

| Source | SHA-256 |
| --- | --- |
| `wic.exe` | `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` |
| `wic_ds.exe` | `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` |
| `local/binaries/server/wic_ds.sdf` | `fd6bbe78096f1f5af77cf66cb08423107b54f8cafb25112ebce7a650bd7086b3` |
| Decoded `maps/supportweapons.ice` | `e046924206a1ed849fb79ce417e4b33579c3812f1712fe338b51ca4df556e94c` |

## Ghidra evidence chain

1. Server `0x0045f1c0` (`EXCO_SupportThing::Init`, identified by its diagnostic
   strings) reads named fields into the support definition: recharge at `+0x2c`,
   activation delay at `+0x34`, marker lifetime at `+0xd0`, marker delay at `+0xd4`,
   initial delay at `+0xd8`, and bridge-repair flag at `+0x1d8`.
2. Server barrage constructor `0x0051bb30` schedules marker creation at
   `now + markerDelay` and initialization at `now + initialDelay`.
3. Server update `0x0051b020` creates the marker with serialized duration
   `markerLifetime + activationDelay` through `0x00683310` and `0x004f85e0`.
4. Client receiver `0x0093fa10` looks up the same support definition and subtracts
   its activation delay before calling `0x008e4160`
   (`WICP_SupportMarkerContainer::AddNewMarker`). That stores the expiry as
   client game time plus the resulting duration. The local placement path at
   `0x008e4700` also passes `definition + 0xd0` directly as marker lifetime.
5. Client update `0x008e44d0` removes expired markers. Countdown renderer
   `0x008e49d0`, called by `0x008e4b10`, uses marker expiry minus game time when
   the fixed display value is negative, and formats the integer seconds modulo
   60. This verifies an actual displayed countdown, beyond a field-name guess.

## Replay timing and completion boundaries

Server `0x0051b020` waits for the initial delay before emitting the ordinary
support-deployment event through `0x004b5220` with age zero. Opponent delivery can
be further delayed, with age measured from that initialization time. Therefore
the ordinary deployment event is not automatically the original call-in time.
The 1-second infantry and 16-second vehicle initial delays explain why matching
windows measured from deployment events differ despite all three having a
35-second ground-marker countdown. Exact spawn times also depend on projectile
and unit-spawner behavior; the marker duration alone cannot prove a landing.

Bridge Repair takes a separate branch: after its 10-second initial delay it finds
a nearby bridge via `0x0051ab70` and calls `0x0051d920`
(`WICG_Bridge::Repair`). That function selects the bridge's faction repair
property, changes its state and reports repair through `0x0051d670`. It does not
wait an additional five seconds before calling repair. This investigation does
not establish a universal bridge traversal/completion timestamp.

The viewer starts countdowns at projected TA row `timeSeconds` and now covers
all recognized top-level TA labels, including Repair Bridge. These are marker
lifetimes, not asserted effect-completion times. The replay-event anchor caveat
above remains, especially for reinforcement drops. Unknown support IDs keep a
brief event label without an invented countdown.

| Viewer label | Marker seconds |
| --- | ---: |
| Aerial Recon | 15 |
| Airborne Infantry | 35 |
| Airdropped Transport | 35 |
| Airdropped Light Tank | 35 |
| Repair Bridge | 15 |
| Napalm Strike | 20 |
| Tank Buster | 12 |
| Laser Guided Bomb | 13 USA/NATO; 11 USSR |
| Air-to-Air Strike | 12 |
| Chemical Strike | 15 |
| Heavy Air Support | 15 |
| Light Artillery Barrage | 10 |
| Precision Artillery | 10 |
| Heavy Artillery Barrage | 12 |
| Airstrike | 20 |
| Daisy Cutter Bomb | 18 |
| Fuel Air Bomb | 18 |
| Carpet Bombing | 15 |
| Nuclear Strike | 16 |

This covers all 58 extracted definitions: both `ClusterBomb` and `Airstrike` map
to Airstrike with the same lifetime, and NATO British nuclear support also uses
16 seconds. Laser Guided Bomb (`Bunkerbuster`) is the only shared label with a
faction-specific lifetime. No AppImage is built automatically by these changes.

## Reproduction

Run from the repository root with the immutable server archive present:

```bash
mkdir -p local/generated/ta-timers
"$HOME/.venvs/wic-analysis/bin/python" research/scripts/ta_timers.py \
  --json local/generated/ta-timers/timers.json
```

The probe decodes the ICE tree using the existing bounded reader, resolves
definition names from shipped localization using Adler-32, and extracts seven
direct timing fields for all **58 top-level faction TA definitions**. It rejects
missing or duplicate definitions and fields, and records source hashes, raw
values, field type hashes and byte offsets. The twelve definitions above provide
positive controls against the user's observed 35-second drop timers.

For code verification, decompile the listed virtual addresses in the matching
Ghidra programs. Local decompilation exports and extracted reports are under
`local/generated/ta-timers/`; copyrighted source data and full decompilations are
not committed. Original replay attribution evidence remains in
[the deployment attribution report](opposing-player-tactical-aid-attribution-2026-08-18.md).
