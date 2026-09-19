# Recorded nuclear detonation map effect

The map animation is anchored to `CreateCloud`, not TA countdown expiry and not
a guessed projectile/deployment join. It is a stylized flash, expanding ring and
four-second fading glow. It does not attribute kills, an issuer or fallout damage.

## Evidence

The stock b35 identities are unchanged from the
[timer investigation](tactical-aid-timers-2026-09-11.md): 32-bit x86,
image base `0x00400000`, `wic_ds.exe` SHA-256
`c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf`,
server SDF SHA-256
`fd6bbe78096f1f5af77cf66cb08423107b54f8cafb25112ebce7a650bd7086b3`.

The existing `wic_ice.support_cloud_types` decoder follows the verified server
load order: `0x00681330` loads support clouds before unit/ammunition clouds,
`0x00680fb0` walks the definitions, and `0x00680f10` appends global types.
Thus `CreateCloud.aType` identifies a catalogue entry independently of a projectile.
`shipped_support_cloud_types()` recovers these nuclear cloud indices:

| Index | Support definition |
| ---: | --- |
| 8 | TacticalNuke_US |
| 18 | TacticalNuke_USSR |
| 28 | TacticalNuke_NATO |
| 34 | TacticalNuke_NATO_British |
| 45 | TacticalNuke_US_NO_MISSILE |

Each is the cloud with key hash **782960095** from the extractor. Each has
140-second lifetime, zero initial logic delay, and the `nuke` sound identifier.
It is created as a projectile death parasite. Its creation record supplies the
animation timestamp and position; its 140-second lifetime is not the animation
duration. The US nuclear definition's separate `PP_BlastDamage` has radius 220
and blast speed 85, used as the illustration's scale and expansion rate. The
flash and fade are viewer presentation choices rather than reconstructed graphics.

`local/replays/main/demo141.wicdemo`, SHA-256
`44ad74079e3ed817929aece3d27680c1d3dacf72d9f013298f442658eca49174`,
provides a positive control at recording time **854.822021484375**, type **18**,
position **[821.28759765625, 38.64686584472656, 1098.462890625]**, lifetime **140**,
heading **0**, team **3**. The lazy Rust playback projection reproduces that event.

## Contract and limits

`PlaybackView.nuclearEffects` adds recording time and world position to the lazy,
uncached playback response. No parser timeline JSON, library database or detail
cache changes; no Refresh library is needed. Reopen the replay to reload playback.
Accept only the complete seven-field CreateCloud shape (119 bytes), exact field
order and types, the known type indices, 140-second lifetime, finite position and
valid faction. Keep at most 16,384 nuclear records, failing closed on overflow.
Unrecognized or malformed records produce no nuclear animation. The catalogue
mapping is for stock b35 data; modified game catalogues are not validated.

The same recording clock drives flash, ring and glow, including seeking and pause.
Reduced-motion preference suppresses the flash and expanding ring. Unit markers
and TA labels stay above the effect. The absence of an effect record never falls
back to countdown expiry. This preserves the earlier
[projectile-join boundary](ta-projectile-simulation-2026-09-01.md): identifying a
recorded cloud is not proof of a projectile ID, player or kill association.
