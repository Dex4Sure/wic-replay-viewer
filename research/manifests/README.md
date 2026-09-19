# Source-evidence manifest

`targets.sha256` identifies the three canonical 32-bit PE targets recovered
from the previous WSL workspace. The files themselves are private,
copyrighted source evidence and are not stored in Git.

Run `research/scripts/bootstrap-local-data.sh` after setting `WIC_DATA_ROOT` in the
ignored `.env` file. The script creates read-only-by-policy local links under
`local/binaries/` and `local/replays/`, then verifies these hashes.

| Target           | Role                      | SHA-256                                                            |
| ---------------- | ------------------------- | ------------------------------------------------------------------ |
| `wic.exe`        | Main client               | `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` |
| `wic_online.exe` | Online client             | `7bb41478f3070641b7639707d72a976a2ae5c8f9ad9356ff5733610979c0b7d0` |
| `wic_ds.exe`     | Dedicated server v1.0.1.1 | `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` |

The recovered corpus currently contains 2,880 `.wicdemo` files across the
main replay collection, WiC settings, and the recovered WiCGate documents
tree. Corpus files are linked locally and never committed.
