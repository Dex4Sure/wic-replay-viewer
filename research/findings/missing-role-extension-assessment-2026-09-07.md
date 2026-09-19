# Missing-role extension assessment — 2026-09-07

**Proposal declined.** The initial-entry-role extension was not implemented. The
user also requested removal of the existing event-derived role fallbacks; retain
this report only as historical investigation evidence.

Read-only investigation of viewer commit `d3fd788`. No parser/viewer changes or
Ghidra annotations were made. At that commit, the experimental fallback covered all explicit
role selections after the named entry and before departure, including the lobby.

## Result

The investigation identified one additional candidate source: the explicit
`role` value carried inside the same named `PlayerEntersGame` message. The proposed design would treat it as
an initial observation, superseded by later selections, while retaining the session
and gameplay guards. It was declined because it resolved only one additional player.
It would have represented a last-recorded role, not a score-derived primary role.

The scan covered all **246 remaining distinct departed-player results**, across
**204 distinct replay contents**, from the completed `/tmp/last-role-audit.json`.
Their slots' entry messages contained 337 zero `role` values and one known role.
Zero does not identify any of the four known playing-role hashes.

The one additional candidate is:

- Replay: `replays/old/2007-11-25_Wilda_DLink_vs_Revoltados_eSports_5on5_Riviera.wicdemo`.
- SHA-256: `0e19c55472251de44e247ccb556784a8e82747b6b470f5aff5b661842b738f5d`.
- Slot 6, `Wilda^Darth`, named entry at recording time 0.100098 s; decompressed
  message offset 2,076, `role` field offset 2,185.
- Initial role value `0x0e0002cb` identifies Infantry. No later `aRoleId` records
  were found for this slot anywhere in the complete decompressed recording.
- The existing verified departure evidence recovers 975 at 1444.000610 s, before
  the same named player leaves at 1444.418823 s. Result time: 1445.610718 s.

## Why extending beyond the session is unsafe

A complete-data scan found 37 additional known `aRoleId` fields among the unresolved
slots: 36 `SetScoreAtGameEnd` fields and one `PlayerSetRole`. Every one followed
an explicit entry naming a **different occupant** from the unresolved result row.

The 36 final summaries are after replacement entries. The remaining selection is
Support at 3.222801 s for `sh.es^Zolad` in slot 5 of
`522__sh.esvsDNxmassfinalr1.wicdemo`; the unresolved departed result is for the later
`D.N^Hunter UK`, who entered at 171.221954 s. Neither earlier nor later occupants'
roles should be carried into the unresolved person's summary.

## Requested examples and positive controls

A complete decompressed-data search for all framed `aRoleId` fields, plus all raw
occurrences of the four known playing-role hashes, found:

- `333__demo17`: COOL GUY (slot 3) and PukinDog (15) have no `aRoleId` records;
  their named-entry `role` fields are zero. HOTWINGS (6) had the experimental fallback's Air
  selection and confirming final summary.
- `demo66`: He_y6uBauTe (4) has no role selection. A later final summary has role
  ID zero and zero per-role scores after That'sGay replaces that slot. Both
  entry `role` fields are zero.
- `demo33`: Cmdr Trigger (1) has no `aRoleId` records; entry `role` is zero.
- Positive controls: `358__demo85` contains the known i.s.a. Support→Infantry,
  HUSSAR001 Armor, and Neuro Support selections. `demo69` contains Donald Duck's
  Support selection. The experimental code at `d3fd788` recovered these roles; the final viewer no longer does.

Known playing-role hash occurrences in the three unresolved examples are all
inside the known `aRoleId` fields; no alternative serialized hash source was found.
This does not rule out a differently encoded, currently undecoded state field.
There is no proven general extension based simply on searching farther in time.

## Binary corroboration

Read-only Ghidra inspection and local SHA-256 verification used `wic.exe`:

- File version `1.0.1.1 (b35)`, x86 32-bit little-endian PE, image base `0x00400000`.
- SHA-256 `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc`.
- Entry-message writer `FUN_00b86b50` serializes player-state offset `+0x260`
  as a four-byte type-0 value under the `role` tag.
- String at `0x00cfdf8c` independently read back as `role`.

This confirms a real serialized player-state field, rather than a label guessed
from unit types. The proposed initial-state fallback was not implemented. Existing event-derived
role recovery was removed in viewer commit `786f2d5`; departure-score recovery remains.

## Derived artifacts

All files below are in this findings directory and contain derived inspection
results or scripts; replay source files remain unchanged:

- `missing-role-timeline-probe-2026-09-07.json`: full role-field records and input
  hashes for the named examples and positive controls.
- `missing-role-entry-corpus-probe-2026-09-07.json`: all 246 unresolved rows' entry
  fields and complete-data role-field matches.
- `missing-role-late-records-2026-09-07.json`: names, entries/leaves, role records,
  and timestamps for every additional-role candidate.
- `missing-role-entry-writer-2026-09-07.json`: read-only decompiler export.
- `inspect-missing-role-timeline-2026-09-07.py`,
  `probe-entry-role-corpus-2026-09-07.py`, and
  `probe-late-role-records-2026-09-07.py`: session inspection scripts, run with the
  viewer repository as cwd. Pass the historical v40 audit JSON as their positional
  argument. It must retain `scoreBeforeLeave.lastRecordedRole`; current score-only
  output is not a substitute. The scripts also need the local message inventory
  and private replay paths referenced by the audit. These inputs and generated JSON
  are intentionally untracked. The bounded entry-name decoder is included in the
  final script, with no dependency on a temporary Python module.

Run the probes in the order listed above, for example:

```bash
cd components/replay-viewer
python ../../findings/inspect-missing-role-timeline-2026-09-07.py /path/to/last-role-audit.json
python ../../findings/probe-entry-role-corpus-2026-09-07.py /path/to/last-role-audit.json
python ../../findings/probe-late-role-records-2026-09-07.py /path/to/last-role-audit.json
```

## Later playback evidence

The user subsequently reported that `CG's epic fail.wicdemo` does not load properly
in-game. It is byte-identical to `333__demo17.wicdemo` (SHA-256
`972959c61587f57dd9cce5d7116c4f0e6ac1ed3c3b65fc55ece9b92860ca0447`).
The parser accepts it and finds 322,557 consecutive event envelopes, including
`TeamWins` within the chain. This demonstrates a gap between the parser's structural
checks and game playback compatibility; the specific playback defect is not yet
established. Treat its bytes as inspectable evidence, not proof of a playable replay.
