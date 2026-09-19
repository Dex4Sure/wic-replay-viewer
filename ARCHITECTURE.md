# Architecture

## Process boundary

```text
Vue 3 + Tailwind             Tauri command/event bridge          Native Rust core
-----------------           ---------------------------         ----------------
Library + filters  invoke → initial_state / replay_detail   →   SQLite lazy cache
Locations + drop   invoke → start_import / remove location  →   bounded coordinator
Progress + rows    event  ← replay-import                   ←   Rayon parser pool
Visible map art    invoke → map_art_state / map_art_batch    →   transactional PNG cache
Replay management invoke → rename / batch export / edit name → verified replay writer
```

The webview never reads replay files or opens SQLite directly. Tauri exposes a
narrow set of typed commands and one typed event stream. The Rust library remains
independent of Tauri so parser, database, and importer behavior can be tested without
a GUI runtime.

`src-tauri/src/command_core.rs` isolates file-operation exclusion, canonical path
validation, and per-path detail-load coalescing from the Tauri runtime. Its portable
tests cover success, failure, panic recovery, retry, duplicate directories, and
distinct concurrent keys. `src-tauri/src/lib.rs` remains the thin framework and IPC
adapter; command names and camel-case payloads are independently locked at the
frontend boundary.

`frontend/appController.ts` owns the portable import-event state machine, replay
removal pruning, and detail-request generation tokens. `App.vue` remains the view
coordinator, but stale async results and streamed import transitions are tested
without mounting the desktop runtime. Tauri command names and camel-case argument
payloads are locked by a table-driven frontend contract suite.

Replay files remain read-only during browsing, import, search, and detail loading.
Only an explicit management action can write one. File rename changes the original's
name within its current folder and refuses an occupied destination. Export
creates byte-identical copies while retaining every source and without overwriting;
a multi-replay export validates every source and destination before its first write
and removes its new copies if a later write fails. It can also create one validated
leaf folder under the selected target; a failed batch removes that new folder after
rolling back its copies. An in-game-name edit locates the unique structural `ReplayName` field,
rebuilds the framed zlib stream, reopens the complete output byte-for-byte, and
requires the production parser to accept the requested name before replacement.
Direct edits use a sibling temporary file and retain the original as a rollback copy
until the validated replacement is in place. Replays without exactly one valid name
field remain readable but cannot be edited in place.

## Import path

The UI starts one coordinator thread per import. It sends one or more persistent
library roots selected through the native picker or native drag-and-drop event. The
coordinator discovers replay paths, deduplicates overlapping roots, checks every
fingerprint against one in-memory snapshot of the current SQLite cache, and runs
uncached work through a dedicated Rayon pool sized to all detected processor threads.
Workers parse match summaries only. Their results cross a bounded channel and the
coordinator serializes database writes in atomic groups of at most 64 summaries. A
batch failure retries those rows individually so one exceptional replay retains the
previous failure isolation. Neither parsed results nor SQLite connections are shared
without bounds. After a non-cancelled scan completes, the coordinator transactionally
deletes cached paths beneath the scanned roots that were absent from the complete
discovery set. It never performs that reconciliation after cancellation or a failed
discovery, and roots outside the requested scan remain untouched.

The frontend updates progress counters once per `summariesStored` event (an array
of at most 64 committed summaries). It buffers rows in a non-reactive path map.
At completion it merges and sorts using 256-row runs and bounded merge steps,
yielding through timer tasks so input and painting can proceed. Before publishing
one shallow-reactive array, it prepares statistics, folder groups, and search
metadata in chunks. Cached projections use weak references to immutable arrays and
rows; replaced libraries can be collected. Empty search bypasses the search index.
The virtualized list still renders only visible rows.

The completion event carries removed paths, allowing ghost rows and affected
selections to be cleared. Generation checks prevent superseded completion work from
publishing after recovery, another scan, or unmount. SQLite remains authoritative:
closing the window does not lose summaries that the coordinator already committed.
Startup and automatic recovery also prepare projections before publishing their
SQLite snapshot. Individual replay-management operations retain the small synchronous
update path; parser output and database compatibility identifiers are unchanged.

The frontend also polls the cheap `import_running` command every two seconds while
monitoring a scan. Once Rust is idle, it reloads authoritative `initial_state` data
from a native blocking worker and reconciles rows and selection. This runs after ordinary
completion too, so missed summary events cannot silently omit committed results.
Failed requests retry; generation tokens discard responses from an earlier scan or
an unmounted window. Callback and event-delivery errors are logged. Recovery requires
a responsive JavaScript event loop; it cannot revive a crashed native webview.

## Storage

`replay_summaries` stores small searchable fields and file fingerprints, including
the nullable in-game `replay_name`, human-readable `server_name`, unique playable
`factions`, and both durations: `duration_seconds` is match length and
`recording_seconds` is replay length. Library search treats whitespace-separated
terms as an AND query whose terms may match different summary fields; it never loads
full replay details while the user types. The library row and the Overview lead with
replay length because that is the length of the file being browsed; match length stays
available beside it. Timeline timestamps live on the replay-length axis from schema
v13 onward, so the two must not be substituted for one another.
`replay_details` stores the complete match-plus-timeline JSON separately. Selecting a
replay loads a current detail row or parses and stores it on a background thread. An
uncached parse projects `DetailView` directly from the typed parser document instead
of serializing and decoding that document merely to build the view. The complete
parser document is persisted alongside viewer-owned `hiddenPlayerIds` and
`playerResultEvidence`, preserving proven duplicate suppression and event-backed
result recovery without rewriting the raw parser document. Concurrent requests for the
same replay path share one in-flight backend load, while a completed or failed key is
removed so a later request can read the cache or retry. Only the selected replay's
decoded detail model is held by the UI.

`map_art` stores runtime-decoded PNG blobs separately from parser summaries and
details. `initial_state` includes the existing art count, so the frontend can enable
cached images as soon as it receives the library instead of waiting for installation
detection or archive validation. The independent `map_art_state` task validates the
configured shipped/custom sources and performs a first decode or stale rebuild in the
background. If any configured source is temporarily unreachable, the last complete
cache remains authoritative; rebuilding from only the reachable subset would discard
valid images. A manual rescan invalidates only the source signature, and the old rows
remain readable until `replace_map_art` commits the replacement transaction.

Visible components queue their unique map names in frontend batches of at most 64.
`map_art_batch` normalizes replay map paths, enforces a larger backend safety limit,
and serves the batch with one SQLite connection and one prepared statement. A true
cache miss is retained as `null`; a command failure receives two bounded delayed
retries and is left unresolved rather than being recorded permanently as missing.
Availability and cache generations are reactive, so tiles mounted before startup
validation are retried when art becomes available, while responses from an old source
cannot repopulate a reset frontend cache.

The cached parser JSON is the raw evidence layer. `DetailView` is a separate display
projection: it resolves player IDs at each event time through schema-v11 participant
sessions, removes raw slot/unit/hash notation from prose, strips WiC markup from
server titles, and retains neutral unknown wording when no identity is available.
The static participant directory remains the compatibility fallback for older
schemas and slots with no session evidence. Once sessions exist for a slot, an
uncovered time is deliberately unknown rather than inheriting a former occupant.

Unit-destruction projection recognizes the nine shipped multiplayer fortification
type hashes as anti-air, anti-tank, or machine-gun fortifications. Destructions with
neither a known participant/team nor a recognized object type remain in cached parser
JSON but are omitted from the timeline instead of producing an unactionable generic
row.
Schema-v17 tactical-aid destructions name the serialized issuing team and support
without inventing a player. Player-owned destructions whose cause remains unresolved
stay visible as neutral unit-loss prose; the raw parser document retains
`cause: unknown`. A same-player killer is described separately as destroying one of
their own units. Unresolved losses are never presented as deletion, ordinary fire,
or tactical aid.

Schema-v18 `destructionContext` is projected independently from attacker identity.
An exact building context reads as a unit destroyed when its occupied building
collapsed; an exact container context reads as destroyed with its transport. These
rows use `UNIT DESTROYED` because their mechanical path is known, but remain neutral
when the initiating actor is not. A tactical-aid team/support appears only when the
parser has inherited independently exact container evidence. Raw building and
container IDs remain in cached parser JSON and are not exposed as user-facing
identifiers.

The Tactical Aid projection joins exact support/position groups from two raw layers:
recorder-visible-faction markers provide player IDs, while top-level delayed-spawn
effects provide both factions. Exact effect keys are traversed in deterministic
sorted order and equal groups are paired in stream order, so equal-time rows remain
stable across operating systems and process hash seeds. For an
unpaired schema-v9 deployment, the nine validated unit-drop definitions may instead
carry a unique `unitSpawnOwnership` player resolved through the event-time identity
session. Other unequal groups, ambiguous drops, and non-unit aids remain team-only
so the viewer never assigns an enemy action to a guessed player.
`aTimeSinceCreation` moves the effect back to its deployment time.

Every row carries `PARSER_CACHE_KEY`. A parser/schema-key change marks summaries
stale and deletes incompatible detail JSON while retaining known replay paths. The
quality gate compares the key's timeline version with the parser workspace member,
preventing older cached details from hiding schema-v9 unit-drop players or
schema-v11 slot-reuse identities. The historical commit component remains in the
current key to avoid invalidating unchanged caches during repository unification;
future output changes advance the schema/detail contract rather than a Gitlink.

Schema-v10 `tacticalAidDamageThreshold` records are projected directly into the
timeline as actor/support/target prose. They are server damage-threshold
notifications, not kill records, so the viewer does not merge them with nearby or
same-time `unitDestroyed` rows. Missing catalogue resolution falls back to the
generic phrase `tactical aid` while the parser retains the raw index.

## UI

The Vue interface is a Vite single-page application hosted by Tauri's platform
webview. The replay library and long Timeline, Chat, and Tactical Aid lists use fixed
row virtualization through `@tanstack/vue-virtual`. Filtering operates on compact
in-memory summaries. Import progress remains live while completed summaries become
visible as one sorted batch at the end of the scan. Persistent library health counts
classify every row exactly once as parsed, failed under the current parser contract,
or stale and awaiting refresh; the status bar separately describes work performed by
the current scan. The application never
materializes detail models for the complete library. Each stored summary includes
the recorder's faction, resolved by
matching the parser's recorder identity to its player record, so the library badge
describes the replay viewpoint rather than the match winner. Older schema-v2
databases migrate the nullable field in place and refresh stale summaries.

`ReplayDetail.vue` owns the playback position for the selected replay path. Leaving
the Replay tab unmounts `ReplayMap.vue` and stops its timer; remounting reads the
retained position without resetting it or starting playback. A path change clears
the position and loaded playback data, and invalidates pending playback requests.

`ReplayMap.vue` renders every projected Tactical Aid row with its support name in
white text on a compact dark background, with faction-coloured borders, dots and
connector lines. Group details also use white text. No TA uses pictorial icons or
expanding rings. Labels use nearby collision-free positions, wrap long names,
and show whole seconds remaining beneath the name for every recognized top-level TA.
The duration table covers every label projected by `support_label`, using shipped
`myMarkerTimeToLive` values. Laser Guided Bomb uses 11 seconds for USSR and 13
for USA/NATO; all other shared labels have the same timer across factions. The
complete values are recorded in the timer investigation. Countdown expiry and
remaining seconds derive from the projected TA timestamp and shared playback
position, so pause, speed changes and seeking need no separate timer. Only unknown,
unmapped support names retain a 1.5-second event fade without a guessed countdown.
The shipped `RadarScan` definitions have zero initial and activation delays;
both marker lifetime and line-of-sight duration are 15 seconds in all factions.
Nuclear Strike and Carpet Bombing have zero initial delay and use the shipped
`myMarkerTimeToLive`; their activation delays are separate. Marker expiry does
not synthesize impacts, damage, destruction or repair completion. White destruction
crosses also fade over 1.5 seconds and do not assert a killer.

The 35-second duration follows the user's in-game test of all three drops and is
confirmed as the game's marker lifetime by the
[Ghidra timer investigation](research/findings/tactical-aid-timers-2026-09-11.md).
That investigation also finds a 15-second Repair Bridge marker lifetime, separate
from the unverified repair-completion time. The viewer's reinforcement timer
anchor is the projected TA row's `timeSeconds`; the countdown has not been matched
to landing in a specific replay. Expiry only removes the pending marker. Unit dots
remain driven independently by recorded `UnitCreate` and position checkpoints,
with infantry squad parents visible and individual soldiers hidden. The parser's
unit-drop ownership matching windows are separate evidence-joining bounds, not
landing timers, and are unchanged. Component regressions cover all named countdowns
across all three factions, the expiry boundary, backward seeking, and a recorded
spawn later than timer expiry. `taLabelLayout.ts` groups competing same-side card footprints after small
horizontal/vertical adjustments fail, rather than using a marker-count threshold.
A frontend-only `groupingCategory` separates the three unit-drop supports from
other TA. Initial grouping and local merges require both category and faction side;
overflow exhausts that consolidation and wider placement before relaxing category
within a side, then faction side. Category participates in the layout cache key.
It then tries a deterministic sequence of
positions inside the canvas, using 180-pixel-wide labels, 46-pixel rows and
six-pixel gaps. Stack heights reserve space for their rows, capped at three rows or
the available map height; overflow scrolls. If a card cannot fit, same-side clusters merge first and placement restarts.
Opposing sides mix only after same-side consolidation and a wider viewport search.
Single-card scoring uses marker-to-card-edge connector length with a bounded
occupancy penalty, starting eight pixels from the marker for ordinary TA. Single
unit drops prefer 24 pixels, then 16, then eight if space requires; grouping
footprints use the same 24-pixel preferred gap. Stacks use the bounds
of all member markers, trying centre/end-aligned positions on four sides at gaps
of 32/48/72 pixels, then 12 pixels, then viewport-clamped fallback positions.
Within each tier they minimize occupied markers/map objects, connector crossings,
and total connector length in that order. The 40-pixel merge preference applies
only to singles. Single cards and stack rows are focusable; hover takes precedence over focus and
highlights the row, exact marker and connector while dimming only sibling lines.
Scroll clears hover, and removed rows or replay replacement clear stale state. Layout
is cached for the active TA membership and viewport, so moving units and timer
ticks do not shift cards; membership changes and resizing resample occupancy. Every active event retains an
exact-position dot and connector. ResizeObserver updates layout on map resizing.
Countdown digits do not affect placement. Groups are always visible with white
text and the same translucent dark card styling as single labels, with blue
USA/NATO borders and red USSR borders, without popup or pinning state. Shared
`replay-ta-card` styling keeps both presentations consistent; both include the
placing player beneath the TA name.
Tests cover crowding, event preservation, bounds, resizing, faction rows and seeking.

Lazy `PlaybackView.nuclearEffects` projects stock b35 nuclear `CreateCloud`
records with exact wire-shape and lifetime validation, bounded to 16,384 events.
It adds no timeline or persistent-cache fields. A four-second map illustration
uses recorded effect time and position; no deployment or player join is inferred.
The ring expands at 85 world units/second up to radius 220, with illustrative
flash/glow fading. All state derives from playback time, not CSS animation clocks;
reduced motion hides the flash and ring. See
[the nuclear effect evidence](research/findings/nuclear-map-effect-2026-09-11.md).

`PlaybackView.areaEffects` also carries validated recorded explosion positions
and radii, and stock napalm/chemical cloud footprints. It is bounded to 131,072
records and stably sorted by recording time. `replayAreaEffects.ts` binary-searches
the last 25 seconds, then applies each individual lifetime. Cloud colours and
explosion flashes are illustrative; generic explosions retain no TA attribution.
This uncached projection requires no database or timeline-schema migration. See
[recorded area effects](research/findings/recorded-area-effects-2026-09-11.md).

The live scoreboard queries `scoreParticipants`, `scoreTeams`, and `scoreSamples`
at that same position. It uses recorded session boundaries and team assignments,
keeps player order stable, and sums the displayed scores only when all are known.
Spectators and players without a known playing team are hidden from this scoreboard. Team cards share Overview's styling;
the winner accent comes from a revealed valid match-result row, never an early
fallback to the final Overview winner. Replay and Tactical Aid event streams share
Chat's table styling, including alternating rows based on their virtual row index.

Replay-time control reporting projects the parser timeline's `dominationSamples`
and `dominationAnchorFaction` through `DetailView`. Both use recording-elapsed time,
the same axis as the playback scrubber and event stream. The frontend selects the
latest sample at or before the current time and holds it until another serialized
sample appears; it does not interpolate the five-second curve. Player recordings use
the timeline's POV anchor. When a spectator recording has no POV anchor, the frontend
matches the final raw sample to the parser's `winnerInferred` final faction shares;
this identifies which faction the unchanged curve represents. A concrete opposing
faction must still be uniquely supported by the final shares or player roster,
otherwise the live meter is omitted. Assault remains excluded because its `aFactor`
is attacker progress, not two-sided control. The replay layout gives the map and live
meter one grid column while the virtualized event stream spans their combined height;
the stream's grid track, rather than a fixed percentage height plus margins, bounds
its bottom edge inside the replay viewport.

Event-stream presentation is deliberately separate from event attribution. Each
`TimelineRow` carries only the exact players named by that event and their team at the
event timestamp. The frontend keeps the row neutral, colors those player-name spans,
and colors literal faction labels. Missing identities and teams stay neutral; the UI
does not choose an event actor or derive a killer or cause for styling.

The Tailwind CSS-first theme uses Wicgate's named colors, square panel geometry,
striped data surfaces, condensed type hierarchy, and red/orange/gold accents.
Neutral action buttons use a common steel-blue hover and pressed treatment, while
primary and destructive actions retain their red treatment.
The four role icons are bundled viewer assets. The header logo is resolved as a
public runtime URL rather than a root-component JavaScript asset import, so a
transient development-server asset miss cannot abort the Vue module graph.

Library cards carry hover, batch-selection, and open-replay state in related
steel-blue borders and surfaces. Batch selection adds a red-filled checkmark, while
the replay currently open retains a narrow red accent. A replay that failed to parse
keeps its warm error border, since that failure is shown nowhere else on the card.
Hover is excluded from selected cards so it cannot override those persistent states;
a `:hover` selector would otherwise outrank the state class on specificity. The
detail tab bar follows the same rule for the same reason: hover stops at the tab
already showing. Any future hover styling paired with a persistent active class
needs that exclusion written out, because source order alone will not deliver it.

Completed Domination and Tug of War overviews project the parser's final faction
percentages into a tactical control meter with viewer-native panel, tick, type, and
faction treatments. Assault is deliberately excluded because its `aFactor` tracks
attacker progress rather than a split between the two sides. A faction whose
displayed control is zero is hidden, allowing a 100% result to render as one
uninterrupted winning segment. The meter is titled for the mode it describes:
Domination is a continuous lead bar, Tug of War a discrete front line.

Segments come from the parser's faction-attributed shares, so the percentage is
independent of the recorded winner. Labels carry two decimals with trailing zeros
trimmed, because the bar resolves to 1/3000 and whole-percent rounding collapses a
real result such as 50.07 / 49.93 into a false tie. Segment widths use the
unrounded values and always total exactly 100.

Match length and recording length are shown separately, and a recording that began
after the match started is called out with the clock time at which the recorder
joined.

Every detail tab draws the selected match's map overview through one shared surface,
reading the same per-map art cache as the library rows. The backdrop therefore adds
no archive decoding and no distinct request for each tab. Readability is enforced in
the styling rather than left to source brightness: the image is blurred, desaturated,
and dimmed beneath a downward-darkening scrim; overview cards and the dense Timeline,
Chat, and Tactical Aid tables retain darker translucent surfaces; and a
`prefers-contrast: more` preference removes the backdrop altogether.

The Tactical Aid tab also consumes the cached map asset's verified terrain bounds.
Those bounds come from the uncompressed size of the effective
`maps/<map>/heightmap.raw` entry. Its square 16-bit sample grid uses three world units
per interval, so a 513-by-513 grid covers 1536-by-1536 world units. Overview and
heightmap entries resolve independently across archive precedence, matching the
game's effective file view. The overview is rotated 180 degrees relative to world
x/z: projection is
`(maxX - x) / (maxX - minX)` horizontally and `(z - minZ) / (maxZ - minZ)`
vertically. This matches the shipped static-radar transform and serialized prop
placements on the overview art. Exact x/z duplicates share a marker; nearby
distinct points are not jittered. Missing, malformed, or ambiguous terrain metadata
disable plotting without affecting the image or the complete Tactical Aid stream. A
single selected row index links the marker layer and virtualized Tactical Aid list;
marker selection scrolls the list, while row selection highlights its map group. A
separate per-faction support-category toggle highlights every matching map group and
dims unrelated markers without filtering or altering the complete stream. The tab
uses the same map-left/event-stream-right composition as Replay: summary facts and
category toggles live above the virtualized deployment cards in the right rail, and a
container-width breakpoint stacks that rail below the map only when the report itself
is too narrow. Selection uses the row background without an additional yellow accent.
Evidenced USA/NATO and USSR player or team names reuse Replay's blue and red text
treatment; unresolved factions remain neutral.

The same runtime map scan reads `maps/<map>/<map>.loc` without bundling game data.
Each `CommandPoint__*` object's `myUiName` is keyed by the Adler-32 object ID stored
in replay ownership events. The Replay event stream substitutes that installed
localized name only for an exact ID match; missing installations, custom maps
without labels, malformed localization rows, and invalid teams retain the generic
parser description. Names are cached with the map asset and follow archive
precedence independently of the overview image and heightmap. The viewer never
uses `myWicedMap.myMissionStats.myName` to resolve a replay's map display name;
that remains parser output.

On Linux, `scripts/tauri.mjs` sets `GDK_BACKEND=wayland` and defaults
`WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` before spawning the Tauri CLI. The native
entry point repeats this policy for direct launches. A September 2026 AppImage
check on the affected mixed-scale Fedora desktop rendered its library through native
Wayland with shared-memory transport. The earlier white-window investigation remains
in `research/notes/replay-viewer-webkit-white-window.md` as historical evidence.
`WIC_REPLAY_VIEWER_GDK_BACKEND=x11` selects the X11 fallback explicitly. Other
platforms are unchanged, and an explicit WebKit transport value remains authoritative.

Development-time file discovery is bounded independently of that rendering policy.
Vite 8's dependency optimizer is given the single `index.html` entry instead of its
default repository-wide HTML glob. Vite's runtime watcher ignores Cargo `target/`
trees and does not follow symlinks, while `.taurignore` independently excludes build,
package, and dependency trees from Tauri's Rust watcher. Together these boundaries
prevent Wine's standard `dosdevices/z:` host-root link from turning discovery into a
scan of `/proc` and the rest of the machine. Tailwind's own source root is explicitly
`frontend/`. These are build-tool boundaries, not parser behavior. Startup opens
SQLite and returns cached summaries plus the cached map-art count; folder discovery
and summary parsing require an explicit scan, full replay detail is loaded only after
selection, and map-art source validation remains a non-blocking background task.

Before viewer projection, the parser applies a targeted final-roster correction
for zero-stat players whose own match-start session explicitly establishes
all-team or one-team spectator status, with no gameplay role selection or unit
creation for the slot. Lobby roles may be superseded by a later spectator event
under the parser's explicit, sole-session inactivity guards. This changes only the result row's team/faction; historical team events,
POV inference, names, and statistics remain intact. The
[parser contract](parser/SCHEMA.md#result-roster-spectator-correction) defines the
exact evidence gates. Zero score alone does not establish spectator status.

When final-screen extraction abstains, or its one-sided roster has an opposing
side in the filtered historical fallback, the importer asks the parser for proven
abandoned lobby duplicate slots.
It filters these from library summary counts/names and Overview, while keeping
the full parser roster in cached detail. A viewer-owned `hiddenPlayerIds` field
beside `replay` and `timeline` preserves the same projection on cache reload;
`detail-v46` invalidates older summaries and details. This requires explicit
pre-game departure, no later slot occupation/activity, and a separately verified
playing entry with the same name and team. It is not a general name deduplication
or zero-score filter. Historical participant sessions and events remain intact.

For that same fallback, the importer queries `player_result_evidence` for scores
reset after explicit departure. It requires named-session and gameplay ownership evidence, preserves
existing results and teams, and stores evidence beside the raw parser document.
Overview marks recovered scores with an asterisk and hover explanation through
`scoreBeforeLeave`. Roles retain the pre-existing parser calculation; neither
last-selected roles nor zero-score role observations are projected into Overview.
Legacy role evidence in old caches is ignored. Unavailable category totals remain
null and are excluded from category leaders. Library matchup calculation uses
the same recovered scores. The
[exact contract](parser/SCHEMA.md#opt-in-event-backed-result-evidence) defines the
departure/reset, signed score, and ambiguity guards. Departure alone does not
establish spectator status.

Player grouping applies a frontend-only presentation policy: only known USA, NATO,
USSR, and Spectator groups are displayed in Overview; Replay shows only playing
teams. Players with unknown or unassigned teams appear under Overview spectators,
with names only and no score or role display. They are excluded from match-leader
lists. This grouping preserves the original faction, team, score, and role values
in the cached parser document and `DetailView`. The parser prefers the highest nonzero signed per-role
score, falling back to an explicit end-summary role ID only for a scored player
whose four per-role totals are zero. In competitive team cards, roles unavailable
from either source render blank; recognized Infantry, Armor, Air, and Support values select their matching bundled icon without guessing unknown roles. Icons
carry accessible role labels and native hover titles instead of duplicating the role
as text below each player. Competitive groups use a fixed geopolitical layout: USA
or NATO is first (left) and USSR is second (right), independent of the match winner.
Paired team cards share a height, and each roster body extends to the bottom border so
its faction treatment also covers empty space caused by uneven player counts. The same
ordering drives the final control meter. The match-leader lists consume
only serialized end-summary totals, per-role scores, and category scores from the
cached parser document. They exclude all players grouped as spectators, omit
categories whose best score is zero, and reproduce the original game's score-only ordering across its fixed 16
player slots. Overall ranks therefore name three individual players, while each role
and score category names only the first eligible player in that game-defined order.
The recorder is identified by canonical name and receives the same orange text
treatment as in the team roster.

## Distribution

The source tree remains one cross-platform Tauri application rather than separate
Linux, Windows, and macOS implementations. Platform packaging is intentionally
split at the artifact boundary:

- Linux releases use an x86_64 AppImage built with Tauri on Ubuntu 22.04.
  The bundle includes the webview dependencies and embedded frontend, and runs
  with normal user permissions. It does not require a separate managed runtime.
  The build baseline bounds the minimum glibc requirement; building locally on a
  newer distribution does not establish compatibility with older systems.
  `scripts/build-appimage.sh` builds through the Tauri CLI, retains native debug
  symbols separately, and finalizes the bundle under a stable release artifact name.
  `scripts/package-appimage.sh` excludes `libwayland-client.so.0` so bundled
  WebKit uses the host library required by its Mesa drivers. Its launcher defaults
  native GTK dialogs to Adwaita dark, preserving `APPIMAGE_GTK_THEME` and `GTK_THEME`
  overrides.
- macOS remains available as a best-effort local Apple Silicon application and DMG
  build using Tauri's ad-hoc signing identity. It requires no Apple credentials,
  but remains unnotarized and may require manual Gatekeeper approval. It is not a
  hosted release target.
- Windows x64 is built natively as one ZIP containing the standalone executable.
  Native Tauri bundling is disabled on Windows, so NSIS and MSI installers are not
  part of the supported distribution. The portable executable relies on an existing
  WebView2 runtime, and release signing remains a separate prerequisite. A
  `cargo xwin` cross-build from Linux exists for local testing only; it is not
  bit-identical to the native artifact, so anything published comes from the
  Windows job.

The Linux application uses its native user data directory for the library database.
Matching `v*` tags build the Linux AppImage and Windows x64 portable ZIP. A tag build
creates or refreshes a draft GitHub Release with those two artifacts but never
publishes it automatically. The Windows executable remains unsigned.

## Reliability invariants

### Validation layers

Portable tests contain synthetic inputs only and run in CI. Tests that need the
copyrighted game installation or private replay evidence are explicitly ignored in
ordinary Cargo runs and fail closed when invoked without their required path. The
private gate resolves replay roles from one tracked relative manifest, validates all
paths first, and then runs the real-install, semantic replay, Quarry playback,
and 17-fixture ground-truth regressions. The slower complete-corpus aggregate
comparison is a separate explicit `quality.sh corpus` audit, not a routine
development or private-fixture requirement.

Coverage is a separate reproducible gate: Vitest uses V8 with 85% line/statement/
function and 75% branch floors, while pinned `cargo-llvm-cov 0.9.0` applies line
floors of 85% to the parser, 80% to this native core, and 75% to the Tauri crate.
Generated reports are not source artifacts.

- At most one import coordinator runs at a time.
- Parser concurrency automatically uses the full detected processor capacity.
- Parsed worker results cross a bounded channel; SQLite writes remain serialized
  and each bounded summary batch is atomic.
- Replay details stay lazy and generation-guarded in the Vue selection flow;
  simultaneous requests for one path share one backend load.
- Cached map art is usable from initial state, fetched in bounded visible-map batches,
  carries heightmap-derived terrain bounds resolved with per-file archive precedence,
  and is preserved across transient source failures or failed refreshes. Frontend cache
  generations reject stale responses after a source change.
- Raw parser identifiers remain in cached JSON even when the display projection
  replaces them with names or neutral human-readable wording.
- The existing platform data directory remains stable. Parser pins and cache keys
  advance together so changed parser/display contracts invalidate derived rows;
  legacy schemas migrate the former single import folder into the current persistent
  location list.

Player result scores and breakdowns use signed 32-bit integers in both the parser
and typed detail projection. Primary roles rank nonzero signed role totals; a
negative total does not outrank positive points, but still records role activity.
Team-score text totals use a wider signed accumulator. See
[signed-score validation](docs/signed-scores-2026-09-07.md).

Result scores prefer the last complete primary-chain `SetScore` before `TeamWins`,
using the message's own `aPos`. Incomplete primary chains retain the established
summary scan. Targeted session/unit evidence can correct a stale lobby name for a
sole pregame replacement or late entrant; roles remain score-derived. See the
[targeted result audit](docs/targeted-event-results-2026-09-08.md).

For historical fallback results, the parser also marks explicitly departed sessions with nullable
`leftAtSeconds`, independently of score recovery. `roster::overflow_departure_ids`
provides the shared presentation policy: only teams exceeding eight omit confirmed
departures, earliest first. Import summaries and typed/cached Overview use the same
filter; fallback cached results and historical playback data retain every participant.
See [departure evidence and validation](docs/departed-roster-overflow-2026-09-08.md).

Overview and import summaries first request `final_screen_players()` from the
parser. Complete final-screen tables supply current occupants, teams, and total/
category statistics together; the older historical roster corrections run only
when that decoder abstains or the fallback restores a missing opposing side. The
whole fallback roster and all player scores are selected for both sides together,
including supported last-known-score recovery, without mixing score tables. Empty Overview cards use
normal faction headers; absent allied names come from result or timeline evidence. Playback and standalone historical parser JSON remain
unchanged. See [the contract and corpus comparison](docs/final-screen-results-2026-09-09.md).
