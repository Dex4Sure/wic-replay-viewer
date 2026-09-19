# Recorded strike footprints

The lazy playback projection now includes `areaEffects` alongside
`nuclearEffects`. It uses effect records independently of the TA deployment
stream. There are no countdown-derived impacts or proximity-based issuer joins.

## Recorded explosions

The previously verified serializers `wic.exe:0x00b826c0` (`SpawnExplosion`) and
`0x00b82590` (`SpawnExplosionWithCrater`) provide recording time, position and
`aRadius`. See the [area-effect audit](unit-destruction-phase-2-area-effects-2026-08-23.md).
The decoder requires the complete seven/eight-field shape, exact tags and types,
finite coordinates and positive finite radius. A 1.5-second flash/ring uses that
radius; animation duration and ring expansion are illustrative presentation.

These records do not identify the weapon or TA. They remain **generic explosions**,
including ordinary combat. Sequences show recorded impact locations and timing,
which can depict bombing runs or barrages without inventing a route, strike type,
or kill attribution. A guaranteed distinction between carpet bombing, artillery,
airstrike and guided-bomb explosions is not available through these records.

## Napalm and chemical clouds

`wic_ice.shipped_support_cloud_types()` recovers stock b35 catalogue indices in
the verified runtime order described in the [nuclear effect finding](nuclear-map-effect-2026-09-11.md).
Input server SDF SHA-256:
`fd6bbe78096f1f5af77cf66cb08423107b54f8cafb25112ebce7a650bd7086b3`.
The same executable identities and addresses documented there apply.

| Effect | USA / USSR / NATO type indices | Radius | Lifetime |
| --- | --- | ---: | ---: |
| Napalm | 0 / 10 / 20 | 14 | 25 seconds |
| Chemical inner | 2 / 12 / 22 | 50 | 25 seconds |
| Chemical outer | 3 / 13 / 23 | 65 | 25 seconds |

Only exact CreateCloud shapes with those indices, a 25-second recorded lifetime
and a valid faction are accepted. Initial logic delay is zero for napalm and one
second for chemical damage; the visualization starts at **cloud creation** and
does not claim damage occurs immediately. Each patch retains its circular radius;
multiple recorded napalm patches produce the area pattern. There is no fabricated
elongated footprint. Colour and end-of-life fade are viewer presentation choices.
Modified game catalogues are not validated.

## Private positive controls

The Rust projection was checked against raw event reads using `wic_bintag`:

| Replay | SHA-256 | First checked effect |
| --- | --- | --- |
| demo139.wicdemo | `29c320db2042006362b7d5e71352e162fb6ae6e8b288694d290a3413d9990fcb` | Chemical type 12 at 515.330078125, position [709.747314453125, 32.50202941894531, 659.7781372070312] |
| demo140.wicdemo | `fabe011b7f48245be921288966929ed640dc2ca13e5e5a8012301dcd6b7c2be9` | Napalm type 20 at 738.254638671875, position [903.7386474609375, 59.39004135131836, 834.9230346679688] |
| demo141.wicdemo | `44ad74079e3ed817929aece3d27680c1d3dacf72d9f013298f442658eca49174` | Explosion radius 6 at 430.462890625; crater explosion radius 35 at 489.376708984375 |

The derived check output is `local/generated/ta-area-effects/controls.jsonl`.
Portable Rust tests cover all nine cloud indices, both explosion families,
wrong lifetime/type, unknown cloud IDs and invalid radii. Frontend tests cover
radius projection, individual impact timing, cloud expiry and backward seeking.

## Bounds and playback

The response is uncached; reopen a replay to reload it without Refresh library.
Parser timeline and database schemas are unchanged. Only full map playback loads
these records, capped at 131,072 events with an explicit overflow error. Events
are stably sorted by recording time. The frontend binary-searches the active
25-second window and applies each event's lifetime, avoiding a full-replay scan
on every frame. Effects render below labels and units with low-opacity fills;
reduced motion hides expanding explosion rings. There is no simulated damage.
