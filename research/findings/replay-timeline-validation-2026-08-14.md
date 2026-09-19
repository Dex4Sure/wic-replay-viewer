# Replay timeline validation — 2026-08-14

> Historical schema note: this report validated the initial schema-v1 timeline.
> The current contract is schema v9; later reports add tactical-aid placements,
> chat and canonical participants, player-bearing markers, spectator/both-faction
> deployments, and validated unit-drop owners. See
> `opposing-player-tactical-aid-attribution-2026-08-18.md` for the current TA
> boundary.

## Scope

The canonical Rust parser in `components/replay-parser` was extended with an
opt-in, replay-relative timeline. Source evidence under `binaries/` and `replays/`
was read only.

## Input identities

| Input | SHA-256 | Purpose |
|---|---|---|
| `binaries/game/wic.exe` | `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` | BinTag event and field names |
| `replays/main/demo01.wicdemo` | `03dbab783aad67b0db4b3f5fffb222cb37d981997370010737462dbdef5eb14b` | Domination clock and event output |
| `replays/main/demo54.wicdemo` | `e83864aed4aeca9b22f7554563e60d462ed7f2db46df557df5ed560e09bf20af` | Two-phase Assault clock reset |
| `replays/main/old/demo44.wicdemo` | `b378fd667d51599c682425bde43a47f733cecbf8ee92d36b9ea9d632d9707cef` | Tug of War clock and mode |
| `replays/main/WicTracker/downloads/961__demo05.wicdemo` | `b92807b8d1b822d5c3bf18192b50ca88d808cc5b9a9104e2befd94915b184963` | Late-recording coverage |
| `replays/main/WicTracker/downloads/777__demo10.wicdemo` | `c88937c1500609db39f1a65a3a520aae87d0e87c0b51413d49f6102bb22b288b` | Negative overtime clock |
| `replays/main/demo144.wicdemo` | `3283339322ef1367e6eabbd7b7b235d1e7e99482e68902b2244c905a8e7eeb13` | Vote event |

## Observations

- Structured `SetGameModeData_Float` records with `aDataType=1` provide the
  gameplay countdown at approximately one-second intervals.
- `demo01` records `1199.899 -> 340.906`, an observed span of `858.993` seconds.
- `demo54` has two approximately 600-second countdowns separated by a positive
  reset, yielding two phases and `1198.128` observed seconds.
- `961__demo05` records only `441.254 -> 400.945`, so its reliable captured span
  is `40.309` seconds rather than a guessed full-match duration.
- `777__demo10` records `-6790.797 -> -6918.081`. Negative values are a valid
  overtime countdown and yield `127.284` observed seconds.
- A deduplicated scan of all linked corpus roots found structured countdown data
  in 2,864 of 2,865 accepted files when restricted to positive values. The only
  apparent miss was the negative-overtime replay above; accepting finite signed
  countdowns gives 2,865 of 2,865 clock coverage.
- The release parser processed every linked replay path. Parser failures were
  limited to the established corrupt fixtures and a separate zero-byte replay in
  a read-only document mirror.

## Validation

- Rust unit tests: 10 passed.
- Rust Clippy with all targets/features and warnings denied: passed.
- WASM-only feature build: passed.
- Python ground-truth suite against its private corpus root: 16 passed, 0 failed.
- Representative native output checked for Domination, Assault, Tug of War,
  late-recording, overtime, and vote-event replays.

## Remaining limits

- Timeline time is observed gameplay time, not wall-clock time.
- Unit type and command-point identifiers remain stable raw hashes/IDs unless an
  exact name is known; the parser does not guess labels.
- Movement, health spam, chat text, and raw unit-create events are intentionally
  excluded from schema version 1.
