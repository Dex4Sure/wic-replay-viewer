# Final-screen fallback recordings — 2026-09-09

The Flatpak corpus audit covered 4,089 paths / 2,499 distinct contents. Of 2,491
accepted contents, 2,483 support the end-screen projection (about 99.7%). Eight
accepted contents use the previous viewer behavior; the eight rejected contents
are a separate set and are not included in this fallback list.

`SetScoreAtGameEnd` supplies the replay screen's statistics. Names and teams come
from recorded player state. A usable score table does not by itself establish a
complete roster, as demo192 demonstrates. No server-result accuracy is claimed.

| Replay | Reason for fallback |
|---|---|
| `demo192.wicdemo` | Final table has 16 rows, but slot 12’s later entry omits its name. |
| `1050__LOLATCOOLGUY.wicdemo` | Earlier compressed data fails its checksum; primary traversal stops before the intact final table. |
| `1077__demo217.wicdemo` | Earlier compressed data fails its checksum; primary traversal stops before the intact final table. |
| `262__demo35.wicdemo` | Earlier compressed data fails its checksum; primary traversal stops before the intact final table. |
| `348__demo01.wicdemo` | Earlier compressed data fails its checksum; primary traversal stops before the intact final table. |
| `277__demo23.wicdemo` | Earlier compressed data fails its checksum; primary traversal stops before the intact final table. |
| `373__demo95.wicdemo` | Earlier compressed data fails its checksum; primary traversal stops before the intact final table. |
| `313__demo419.wicdemo` | Earlier compressed data fails its checksum; primary traversal stops before the intact final table. |

## Reproducible identities

These are the exact inputs from `final-screen-comparison-2026-09-09.json` and
`final-screen-candidate-corpus-2026-09-09.jsonl`. The latter records the validated
primary result offset, decoded summary rows, and identity state. Both bulk files
are ignored derived evidence. `check-final-screen-2026-09-09.py` reproduces the
comparison; `verify-final-screen-2026-09-09.py` independently checks all eight
fallbacks among its 32 Python/native controls.

### demo192.wicdemo

SHA-256: `02223e6fb3df9028108818bff446257587ffaaaa163e34f9fd78bb20bdac332a`

Library paths:

- `<private-evidence-root>/replays/old/demo192.wicdemo`
- `<private-evidence-root>/Replay Old/demo192.wicdemo`

Primary result offset: `25029126`; decoded final-score rows: 16.

Slot 12 is present on recorded team 3, but its later entry omits the name field.
The type field is present (`myType=0`); the original decoder could not locate it
without the preceding name field. The whole final-screen candidate abstains; it does not replace only
that row using guessed identity.

### 1050__LOLATCOOLGUY.wicdemo

SHA-256: `5c294f7d44bccb8c4bab4419a04dca21bd2ff5bf68859b19bb1f728563784f98`

Library paths:

- `<private-evidence-root>/replays/WicTracker/downloads/1050__LOLATCOOLGUY.wicdemo`

The original primary-chain probe did not reach the result; see the independent
table extraction below.

### 1077__demo217.wicdemo

SHA-256: `f2ef73562759faea788176fdf217fa51a3349bfae37ca78287253a159f89d812`

Library paths:

- `<private-evidence-root>/replays/WicTracker/downloads/1077__demo217.wicdemo`

The original primary-chain probe did not reach the result; see the independent
table extraction below.

### 262__demo35.wicdemo

SHA-256: `b184030754ce7155864692e89aa7c5f1b5dd6edaef163c3be97e1c6d780ccb38`

Library paths:

- `<private-evidence-root>/replays/WicTracker/downloads/262__demo35.wicdemo`

The original primary-chain probe did not reach the result; see the independent
table extraction below.

### 348__demo01.wicdemo

SHA-256: `99a16d9ced813471004a35d762dc67ec6b2b117937b0698b5ad5a4b7c675e79a`

Library paths:

- `<private-evidence-root>/replays/WicTracker/downloads/348__demo01.wicdemo`

The original primary-chain probe did not reach the result; see the independent
table extraction below.

### 277__demo23.wicdemo

SHA-256: `a677da2c299b2a6a24517ca4609c8a24f9de398ebef4bd411dbfa21d4b85e61d`

Library paths:

- `<private-evidence-root>/replays/WicTracker/downloads/277__demo23.wicdemo`

The original primary-chain probe did not reach the result; see the independent
table extraction below.

### 373__demo95.wicdemo

SHA-256: `9387c66ed7fa9f4b1a58fe76cc5ec60681e1471e936ad404a7711d72b6066d94`

Library paths:

- `<private-evidence-root>/replays/WicTracker/downloads/373__demo95.wicdemo`

The original primary-chain probe did not reach the result; see the independent
table extraction below.

### 313__demo419.wicdemo

SHA-256: `2fc6ccc473605d67ce924fbf8eaf4b7324f3339d76dbee02b1df143d99d570cf`

Library paths:

- `<private-evidence-root>/replays/WicTracker/downloads/313__demo419.wicdemo`

The original primary-chain probe did not reach the result; see the independent
table extraction below.

## Interpretation and limits

The follow-up below confirms readable final-score tables outside the seven
incomplete primary chains. This does not establish reliable final occupant
identity or game playback. The existing fallback preserves their previous treatment. Demo192 has two library copies, so these eight contents
correspond to nine paths.

The original report did not change fallback selection; the follow-up below
records the subsequent extraction and continuity checks. The installed candidate
and its validation are documented in
[the viewer report](../../docs/final-screen-results-2026-09-09.md).
The previously reported Flatpak GUI freeze remains a separate unresolved issue.


## Focused follow-up: final data is present in all eight

The input hashes above were rechecked. Independent scanning found correctly typed,
259-byte final-score envelopes, unique slots, and a contiguous table followed
immediately by a framed `TeamWins` in every recording. Each entire table and result
fits inside one checksum-valid zlib chunk.

| Replay | Final rows | First failing zlib offset in original file |
|---|---:|---:|
| `demo192.wicdemo` | 16 | Not implicated |
| `1050__LOLATCOOLGUY.wicdemo` | 2 | 2099925 |
| `1077__demo217.wicdemo` | 10 | 952909 |
| `262__demo35.wicdemo` | 5 | 1944365 |
| `348__demo01.wicdemo` | 16 | 382475 |
| `277__demo23.wicdemo` | 15 | 4707967 |
| `373__demo95.wicdemo` | 2 | 721736 |
| `313__demo419.wicdemo` | 10 | 2493975 |

All seven WicTracker failures report zlib `incorrect data check` at the first
missing compressed stream. Concatenating later successful chunks breaks event
framing; in demo217 it splices an envelope timestamp into a tiny finite value that
looks like a recording-clock reset. Bypassing the checksum in a diagnostic
raw-DEFLATE probe did not restore a clean primary chain in any of the seven.
Those diagnostic bytes are not trusted evidence and were never written to inputs.

In demo192, slot 12's named entry occurs at 15.976 seconds, its departure at
699.277 seconds, and its nameless re-entry at 735.440 seconds. The later entry
contains a framed `myType=0`. Reusing the earlier name would assume continuity
across explicit slot departure and reuse; the recording evidence examined does
not establish that continuity.

The implementation separates table decoding from occupant validation and retains
all eight fallbacks. It also records decompression splice offsets so damage cannot
be hidden by accidental alignment of the remaining event stream. No new parser
JSON fields are introduced. Synthetic Python/native regressions cover aligned
splices, damage within a table, post-result damage, and nameless slot reuse.
Detailed byte probes and reproducible scripts are kept as ignored derived output
under `local/generated/fallback-investigation/`.


Focused validation of the implementation compared the current native parser and
Python reference with the saved candidate baseline on 32 SHA-256-verified controls.
All eight fallback cases retained fallback while exposing their intact table to
internal inspection; the other 24 retained exactly the same final player rows and
statistics. Native and Python table counts and decompression splice offsets agreed.
The machine-local results are in
`local/generated/fallback-investigation/current-validation.json`.
