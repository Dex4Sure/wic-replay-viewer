<p align="center">
  <img src="public/wicgate-logo.svg" alt="Wicgate" width="220">
</p>

# WiC Replay Viewer

An offline desktop application for browsing and inspecting _World in Conflict_
`.wicdemo` replays. Search your replay collection, inspect match results, and
follow recorded battles on a 2D tactical map with chat and Tactical Aid events.

The viewer presents identities, events, and outcomes when the replay provides
enough evidence. Unresolved data stays unknown. A game installation is optional;
your own game files enable map overview artwork and installed map labels.

**[Download the latest release](https://github.com/Dex4Sure/wic-replay-viewer/releases/latest)**
· [First use](#first-use) · [User guide](docs/viewer-guide.md)
· [Build from source](#build-from-source)

## Downloads

| Platform          | Download                                                                                                                        | Run                                      |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| Windows 10/11 x64 | [Portable ZIP](https://github.com/Dex4Sure/wic-replay-viewer/releases/latest/download/WiCReplayViewer-windows-x64-portable.zip) | Extract and run `wic-replay-viewer.exe`. |
| Linux x86_64      | [AppImage](https://github.com/Dex4Sure/wic-replay-viewer/releases/latest/download/WiCReplayViewer-linux-x86_64.AppImage)        | Make it executable and run it.           |

Windows normally includes the required WebView2 runtime. The executable is
unsigned, so Windows may show an unknown-publisher warning.

On Linux:

```bash
chmod +x WiCReplayViewer-linux-x86_64.AppImage
./WiCReplayViewer-linux-x86_64.AppImage
```

macOS currently has local source builds only.

This README describes the development version on `main`. For the changes included
in a download, read its release notes; newer work is listed under `Unreleased` in
[CHANGELOG.md](CHANGELOG.md).

## What you can do

- **Browse a replay library.** Add multiple folders, scan recursively, and search
  by players, factions, maps, dates, server metadata, and match format.
- **Inspect match results.** View teams, player roles, final control, and scores,
  with conservative fallback when the final result cannot be established.
- **Follow the battle.** Seek and change playback speed on a 2D map showing recorded
  unit states, command points, destruction, and Tactical Aid deployments.
- **Read events in context.** Browse chat and replay events alongside the map and
  follow player scores on the playback clock.
- **Manage local replays.** Rename files, edit the in-game replay name, or export
  byte-identical copies through explicit management actions.

Scanning and browsing leave original replays unchanged. Imports are cancellable;
the viewer caches the library locally and loads full details when you select a
replay. See the [user guide](docs/viewer-guide.md) for search syntax, result
selection, Tactical Aid timers, and playback behavior.

## First use

1. Launch WiC Replay Viewer.
2. Select **Add replay folder**, or drop folders onto the window.
3. Select **Scan new folder** or **Scan library**. Adding a folder does not start
   parsing automatically.
4. Select a replay to open its Overview, Chat, Replay, and Tactical Aid views.
5. Optionally open **Map art** in the library sidebar and select the game folder
   containing `wic.exe` and the shipped `wic*.sdf` archives. Configure downloaded
   maps separately if needed.

The viewer checks common Steam, GOG, Ubisoft, Wine, and Proton game locations on
first run. Without game files, replay parsing and procedural map tiles still work.

Try `generalx nato riviera` to find GeneralX playing NATO on Riviera. For more
examples, see [searching](docs/viewer-guide.md#searching). After upgrading an older
library, use **Refresh library** once to update its cached search data.

## Limits and local data

Playback shows recorded checkpoints; it does not recreate the original game
simulation or interpolate unit movement. Corrupt or incomplete replays may be
rejected or leave parts of a match unresolved. Tactical Aid events do not prove
that the aid caused a nearby unit destruction.

Your replay files stay in their original folders. The local database stores paths,
derived replay data, and decoded map art. Rename and in-game-name edits require an
explicit management action; exports preserve their sources. Game binaries, replay
corpora, and map textures are not distributed with the project.

Error dialogs can generate a local technical report with your consent. Nothing is
uploaded automatically. See [local data](docs/viewer-guide.md#local-data-and-game-files)
and [error reporting](docs/error-reporting.md) for details.

## Build from source

Use Node.js 24, the Rust version selected by `rust-toolchain.toml`, and the
[Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).
From a checkout of this repository:

```bash
npm ci
npm run tauri dev
```

The application uses Tauri 2, Vue 3, TypeScript, and a Rust core with SQLite.
The replay parser is embedded in the same versioned product.

See [contributing and building](docs/contributing.md) for platform builds, Linux
rendering settings, quality gates, coverage, and release preparation. Before
publishing changes, configure the documented Python/Ruff tools and run:

```bash
./scripts/quality.sh full
```

The portable gate uses synthetic fixtures and does not require private replays,
a game installation, or Ghidra.

## Documentation

- [User guide](docs/viewer-guide.md) — search, results, playback, and local data
- [Contributing and building](docs/contributing.md) — development, tests, and releases
- [Architecture](ARCHITECTURE.md) — native/frontend boundaries and reliability
- [Changelog](CHANGELOG.md) — released changes and current development
- [Parser documentation](parser/README.md) and [JSON schema](parser/SCHEMA.md)
- [Research](research/README.md) — format investigations, tools, and evidence setup

## License

Licensed under the [MIT License](LICENSE). Third-party notices are available in
[THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt).

## Legal

WiC Replay Viewer is an unofficial community project and is not affiliated with
or endorsed by Ubisoft or Massive Entertainment. _World in Conflict_ and related
trademarks belong to their respective owners. This statement is also distributed
with packaged builds in [NOTICE.txt](NOTICE.txt).
