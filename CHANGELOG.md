# Changelog

## Unreleased

## 0.6.3 - 2026-09-22

### Highlights

- Rank each team's live replay scoreboard by the players' current scores.

### Fixed

- Sort players within each live replay scoreboard team by their current score,
  highest first, while keeping scores that have not yet been recorded at the end.

## 0.6.2 - 2026-09-19

### Highlights

- Make the repository easier to set up and continue with a different replay
  collection through portable contributor and research documentation.
- Use native Wayland by default on Linux while retaining an explicit X11
  fallback.
- Include the project license, third-party notices, and legal notice in packaged
  builds.

### Changed

- Add a concise legal notice identifying the viewer as an unofficial community
  project and acknowledging the game's related trademarks, and include it in
  packaged builds.
- Separate durable agent instructions from host setup, contributor guidance,
  and historical research records so repository policy remains portable.
- Make research paths portable and document how contributors can work with
  replay collections other than the original private corpus.
- Default the Linux viewer to native Wayland, retaining an explicit X11 fallback
  after the Wayland AppImage rendered successfully on the affected Fedora desktop.

## 0.6.1 - 2026-09-17

### Fixed

- Colour live replay map units and perimeter points from their fixed team IDs
  (USA/NATO blue, USSR red) instead of the result roster, so spectator recordings
  whose roster omits one side no longer draw that side white.

## 0.6.0 - 2026-09-13

### Highlights

- Follow battles with clearer Tactical Aid cards, verified countdowns, exact map
  positions, and recorded nuclear, explosion, napalm, and chemical effects.
- Download Linux releases as an x86_64 AppImage; Windows remains available as a
  portable ZIP.
- Generate richer local diagnostic reports for freezes and unexpected exits while
  retaining explicit consent and local-only storage.

### Added

- Document a Tactical Aid taunt/health sequence that selects 1,808 unknown-loss
  candidates across 100 distinct replays, with 411 agreeing existing TA controls.
  Preserve the evidence and remaining proof gaps; parser attribution is unchanged.

- Extend local error reports to schema 2 with bounded freeze/recovery/exit
  timelines, numeric exit codes and Unix signals, active tab/focus and pending
  operation context. Capture later lifecycle evidence when an open warning is
  saved, preserve terminal notifications behind queued warnings, and keep schema 1
  reports readable. Reports remain local and require per-incident consent.
- Add explicit Linux native-stack capture for live freezes. Verify capture and
  detach on a synthetic process; no automatic stack or memory-dump collection.
- Verify the updated Windows helper under Bottles/Wine: dismissal saves nothing,
  generated reports retain freeze/recovery/exit code 42 and UI context, and native
  read-only previews open. Cross-build the Windows release executable; full
  WebView2 and Windows-host runtime validation remain outstanding.
- Ignore `src-tauri/errors/` and all its contents in Git and formatting checks for
  developer-held report exports; application report storage is unchanged.

- Show recorded explosion flashes and rings at their individual positions and
  radii, plus lingering orange napalm patches and green chemical clouds. Follow
  replay time and verified effect lifetimes without guessing the originating TA.

- Animate recorded nuclear effects on the replay map with a brief flash,
  expanding shockwave and fading glow. Follow recorded effect creation rather
  than countdown expiry, including pause and seeking; respect reduced motion.

- Keep crowded Tactical Aid labels readable with nearby placement and connector
  lines to exact map positions. Group competing card footprints after small local adjustments fail, keeping
  opposing sides separate wherever possible. Keep uncrowded connectors short and
  prefer local stacks over spreading cards along long connector lines. Place
  stacks outside their full marker area with a larger gap; hover or focus a single card or stack row
  to highlight its exact marker and line without opening a panel. Keep unit-drop
  stacks separate from other TA unless space forces a fallback, and give single
  drop cards a larger preferred gap from their markers. Prefer clear map space and limit stacks to three
  visible rows, with stable placement during countdowns. Keep individual
  names, players and countdowns always visible. Single cards also show the placing
  player; both use matching translucent dark backgrounds, white text and blue
  USA/NATO or red USSR borders. Large stacks scroll without a hover panel.

- Recover Tactical Aid marker countdowns from shipped definitions and Ghidra:
  35 seconds for the three reinforcement drops and 15 seconds for Repair Bridge.
  Add a reproducible extractor for all 58 top-level faction TA definitions and
  document deployment-event delays separately from marker lifetimes.

### Changed

- Reorganize the README around downloads and first use, document the v0.6.0
  AppImage, retain the v0.5.0 Flatpak as a historical download, and move detailed
  user and contributor guidance into dedicated documents.

- Complete the research history cleanup on main, reducing its ancestry from 482
  to 243 commits at cutover while preserving application files and published binary
  assets. Retain verified Git backups and commit mappings, make historical migration
  verification archive-based, and record the successful CI and completed Fedora
  and Mac checkout handovers. Remove the obsolete research refs and review branch.

- Show every Tactical Aid as a compact white text label on a dark
  background, with shipped marker countdowns for all recognized TA types including
  Repair Bridge. Replace the previous icons and expanding rings. Respect the USSR
  Laser Guided Bomb's 11-second timer versus 13 seconds for USA/NATO. Timers follow
  pause, playback speed and seeking; unknown support IDs retain a brief event label
  without a guessed duration. Unit dots remain tied to recorded spawns.
- Switch Linux downloads to AppImage starting with v0.6.0, alongside the Windows
  portable ZIP. Remove the Flatpak build and runtime setup. Keep diagnostic symbols
  separate from downloads and build Linux releases on Ubuntu 22.04 for compatibility.
- Require an explicit release request and keep public release notes to a short
  overview of user-visible changes.

### Fixed

- Remove the hover background from non-interactive chat messages.

- Fix AppImage startup on Fedora by using the host Wayland client library, avoiding
  a conflict with newer graphics drivers that terminated the webview.
- Keep AppImage file pickers and error-report dialogs dark to match the viewer,
  while respecting explicit theme overrides.

## 0.5.0 - 2026-09-10

### Added

- Add an independent error-report supervisor that survives application-child exits,
  including inside Flatpak. Use native system dialogs for crashes, sustained freezes,
  and unexpected application errors. Ask permission before generating each local
  report; keep diagnostic context in bounded memory until consent, with no automatic
  reports, disk checkpoints, or report footer. Provide native preview and explicit
  export, exclude private replay data/messages/paths, and retain developer symbols
  separately. Group ongoing stall notifications and require separate confirmation
  before closing an application. Consent applies to one incident; saved reports
  remain the approved snapshot. Unsaved context is lost if both processes exit,
  with no next-launch recovery checkpoint. Detect a stopped diagnostic stream
  without inferring a UI freeze. Verify consent, dismissal, whole-app suspension,
  and Flatpak child-exit handling; controlled probes do not establish the original
  freeze's cause.

- Add fast library filters for player, map, and faction, including quoted
  names. Player/faction filters match the same selected result occupant. Prepare
  search metadata once per summary and parse each query once per change; keep
  ordinary text search and lazy detail loading. Database schema 13 stores player
  associations and marks older summaries for a one-time **Refresh library** without
  discarding details. Add collapsible Search tips and a reproducible benchmark;
  verify the player/faction filters against the GeneralX/Riviera recordings.

### Fixed

- Count zero-score players in match formats for every server mode, removing the
  separate Clan Match exception. Use the selected result roster's teams while
  retaining spectator exclusion, unknown-team fallback, and duplicate/departure
  cleanup. Refresh summary and detail projections with cache `detail-v48`.
  The 4,089-entry library audit identified 274 affected entries (185 distinct
  files); zero score alone is no longer treated as evidence of nonparticipation.

- Combine player names, factions, and maps in ordinary library searches:
  `generalx nato riviera` matches GeneralX playing NATO on Riviera regardless of
  recorder. Require stored player/faction evidence while retaining server, format,
  date, and other metadata terms. Remove the winner filter, keeping match results
  for display. Update search tips and documentation.

- Make the portable quality gate work with macOS temporary-path aliases and BSD
  `find`. Count replay paths safely even with embedded newlines, isolate concurrent
  parser CLI test directories, and include stderr in CLI contract failures.
  Document Python and Ruff overrides for hosts with an older system Python.

- Split bulk library merge/sort and search, statistics, and folder preparation into
  yielding chunks before a single shallow-reactive publication. Keep completion
  marked active until preparation finishes and discard obsolete asynchronous results.
  Verify synthetic 10,000-row parity and UI yielding without private replay inputs.

- Batch import notifications into at most 64 committed summaries per event. Poll
  lightweight backend import state and reload saved results after completion, even
  when row/completion events were missed. Retry failed recovery and reject stale
  responses; keep SQLite snapshot work off the native event thread.

- Keep historical replay results when compressed data was skipped before the
  final screen, even if the remaining event stream happens to align. Decode final
  score tables separately from occupant identity, retain fallback for unnamed
  replacement players, and refresh cached projections with `detail-v47`. All eight
  investigated extraction failures contain intact score tables but retain fallback
  because final-player evidence is unreliable. Validate against 32 replay controls,
  the full quality gate, and all 17 private ground-truth fixtures.

- Preserve recorded departure-time precision when loading cached replay details,
  so fresh and cached player projections match exactly. Existing caches remain
  readable without reparsing; roster and score selection are unchanged.

### Changed

- Rename the host analysis environment to `~/.venvs/wic-analysis` and update
  research setup instructions and quality-tool fallbacks. The separate Ghidra
  MCP environment is unchanged. Refresh migration documentation with the completed
  merge, standalone setup, and retained backup locations.

- Integrate the WiC research workspace and its history under `research/`. Unify
  instructions, hooks, and portable checks; use ignored `local/` evidence paths
  without a parent checkout. Product behavior and version are unchanged.

- Restore the complete historical fallback when an end-screen roster loses an
  opposing side that the fallback still contains. Use fallback players and scores
  for both sides together, retaining supported last-known-score recovery and its
  asterisk; never mix final-screen and fallback scores. Refresh cached results.

- Keep empty Overview team cards with their normal faction name and no placeholder
  players. Resolve missing allied names from result or recorded timeline evidence.

- Prefer replay end-screen players and totals in Overview and library summaries.
  Use the same final statistics as Player Performance, omit departed occupants,
  and bypass historical score/roster recovery when a complete final screen is
  selected. Keep the existing fallback for incomplete records and the missing-side
  exception above; refresh caches.
  Document the eight fallback recordings: seven incomplete primary event chains
  and demo192’s undecodable slot-12 identity. Final-screen authority applies to
  replay statistics, not server-reported results.

- Highlight the recorder’s name in the Replay scoreboard using the same orange as Overview.

### Known issues

- A frontend freeze was reported during the installed Flatpak library refresh.
  Stored summaries completed updating. Event batching and automatic recovery now
  address identified weaknesses; the original installed-Flatpak hang still needs
  a private-corpus runtime retest.

### Fixed

- Mark explicitly departed result players without discarding their parser statistics.
  In Overview and library summaries, omit the earliest confirmed departures only
  when a playing team exceeds eight rows, correcting demo58's cumulative 7v9 roster
  to 7v8. Preserve smaller teams and historical playback; refresh cached results.

- Read final scores from each complete score event's own player slot, capturing
  missed final updates and preserving signed scores and zero resets. Keep the
  established summary fallback when the primary event chain is incomplete.
- Correct stale lobby names for proven sole gameplay occupants, including
  Real-Escobar in demo03, without transferring scores between shared slots.
  Existing departure-score recovery now applies to those corrected identities.
- Recognize one-team spectators and explicitly inactive sessions whose old lobby
  role was superseded by a spectator event. Preserve actual participants and
  score-derived roles; refresh cached library summaries and details.

- Preserve negative player scores and score breakdowns instead of displaying huge
  unsigned values, including Weed Queen's -11 in 799__demo03. Sort results and
  sum team scores correctly; select primary roles by the highest nonzero signed
  role score while retaining negative-only role evidence and existing player
  identities. Refresh cached summaries and details so old values are replaced.
- Restore a proven departed participant's last recorded score when the slot resets
  to zero, with an asterisk and hover explanation. Preserve raw results, identities,
  teams, and score-derived roles; leave unavailable category statistics
  unknown. Apply the same score recovery to library matchups and cached details.
  Remove the experimental zero-score and last-selected-role recoveries. Leaving
  never implies spectating.
- Recognize all-team spectator switches before gameplay even after lobby role
  selections, including the recorder in 730__demo95. Require zero final and summary
  scores, and preserve players with unit activity or gameplay role/team joins.
- Hide proven abandoned lobby duplicates from Overview and library counts/names,
  including Hunter UK's zero-score slot in NoW vs Shiny 3. Require explicit
  departure before gameplay and a verified playing counterpart; preserve its
  score and role, the parser's complete roster, and historical events. Keep the
  same visible roster when reopening cached details.
- Correct zero-score result rows with explicit all-team spectator evidence for their
  own match-start session, including Default and JMann9 in demo263. Preserve players
  with gameplay role selections, units, or nonzero statistics, and leave ambiguous
  cases and unknown teams unchanged.
- Fill a missing role for a scored player from the same end-summary block's explicit
  role ID when all per-role totals are zero, including Small Island in demo186.
  Preserve primary roles derived from nonzero per-role scores.
- Correct stale lobby names when a scored slot was vacant at gameplay start and
  has one later entrant with a recorded team matching its existing result and a
  primary role established by nonzero per-role scores. Preserve scores, roles, teams,
  and ordinary roster resolution.
- List players without a known team under Overview spectators, showing names only
  and excluding their scores and roles from match leaders. Prevent unknown factions
  from appearing as extra Replay scoreboard teams.
- Correct Recorder view when a player spectates in the lobby and joins a playing
  team before gameplay. Discard superseded lobby spectator states and clear them
  across session boundaries. Recorded in-game team joins combined with spectator
  states display “Player / spectator POV”. Refresh cached replay data on upgrade.

## 0.4.0 - 2026-09-05

### Added

- Show live player scores grouped by team above the Replay event stream, including
  summed player scores, signed penalties, and synchronized forward/backward seeking.
  Missing observations remain unknown, and reused player slots do not inherit
  earlier occupants' scores or teams. Team rosters sit side by side without
  internal scrolling, with stable player order and more room for the event stream.
  Spectators and players without a recorded team assignment are hidden.

### Fixed

- Preserve the Replay playback position when switching detail tabs, pausing while
  away. Selecting another replay resets playback to the beginning.
- Merge duplicate saved replay folders after Windows path canonicalization and
  show familiar folder paths without the extended-path prefix.

### Changed

- Keep Tactical Aid categories visible in compact faction cards above the scrolling
  deployment stream, with aligned names and counts and a condensed metadata strip.
  Category selection continues to highlight map markers without filtering events.
  Remove the player-scoped Cost POV and deployment cost readouts from this view,
  and shorten coverage wording to “Includes: Both teams” with details on hover.

- Unify Replay and Tactical Aid with Overview and Chat: shared team cards,
  faction header gradients, table headers, alternating rows, and map framing.
  Live rosters remain compact and fully visible, and event selection stays distinct.
  The winning live team gains Overview's orange top accent when playback reaches
  its recorded match-result event; rewinding before the result removes it.
- Focus hosted release builds on the Windows x64 portable package and Linux
  Flatpak. macOS remains available as a best-effort local source build without
  consuming routine GitHub Actions capacity.

## 0.3.4 - 2026-09-04

### Fixed

- Keep opposing team roster cards aligned when the teams have different player
  counts, including the full USSR red treatment down to the shared bottom edge.

### Downloads

- Provide focused downloads for Windows x64, Linux Flatpak, and Apple Silicon
  macOS.
- Use descriptive package names so downloaded files remain easy to identify:
  `WiCReplayViewer-windows-x64-portable.zip`,
  `WiCReplayViewer.flatpak`, and `WiCReplayViewer-macos-arm64.dmg`.

### Maintenance

- Update the GitHub release tooling to its current runtime, removing the deprecated
  Node.js 20 workflow warnings.

## 0.3.2 - 2026-09-04

### Added

- Show the packaged application version at the right edge of the status bar. It
  follows the synchronized release version automatically and retains that same
  value in plain-browser development where the Tauri runtime is unavailable.

### Changed

- Keep hosted Actions demand-driven. Remove the scheduled cross-platform release
  cache warmer and the release-tag Rust cache plumbing, which spent private-repo
  Actions allowance even when no release was planned. Ordinary pushes retain their
  conditional on-demand Rust cache; release packages build only for an explicit tag.
- Simplify tagged releases by folding exact-commit quality verification into the
  precondition job, failing immediately when the required checks are absent, and
  collecting all package artifacts with one download. Remove manual all-platform
  dispatches and tag-scoped frontend-tool caches that later releases could not reuse.
- Split the push gate into two parallel jobs and skip the Rust half when no Rust
  changed. The single job spent about 130 of its 201 seconds on a 925 MB Cargo
  target cache, the WebKit development packages, Clippy, and Rust coverage, and
  paid all of it on commits that touched only documentation, workflows, or the
  frontend. `quality.sh` gained `ci-portable` and `ci-rust`; the local hooks still
  run `fast` and `full` exactly as before, including full LLVM coverage. A commit
  that does touch Rust still runs both hosted halves, so a bypassed local hook
  cannot skip the workspace tests or strict Clippy gate.
- Ensure the locked crate registry from both CI halves. `cargo fetch` sat in the
  portable half only, so the Rust half drove Clippy and tests against whatever the
  restored Cargo cache happened to hold, and `licenses:check` depended on ordering
  rather than on a stage of its own.
- Stop repeating the portable quality gate during a release. The release workflow
  gated every build on a second full run of the gate that had already run on
  `main` for the same commit. The expensive builds now start behind a fast
  release-precondition check, while `Require portable quality` confirms in parallel
  that `Portable quality` actually succeeded for the released commit and gates
  `draft-release` on it. Nothing is published without a green gate, and the release
  is verified against a real run instead of being assumed.
- Keep full LLVM coverage in the local pre-push gate and make hosted Rust CI run
  the workspace tests plus strict Clippy instead of recompiling the workspace a
  second time under instrumentation. Release publication now checks that both
  the portable and conditional Rust jobs succeeded for the tagged commit rather
  than accepting only the workflow's overall conclusion.
- Restore Linux build dependencies through ordinary APT after the package-cache
  action silently treated obsolete-package 404 errors as success and left the
  Rust job without GLib or WebKit headers. This also removes that third-party
  action from the trusted CI surface.
- Replace the push gate's immutable coverage-era Rust cache and retain workspace
  crate artifacts in its successor. The previous exact cache hit restored 882 MB
  but could not save the Clippy/test outputs produced afterward, forcing the
  Tauri workspace crates to rebuild for roughly four minutes on every Rust run.
- Supersede in-flight pull request quality runs, and prevent two builds for the
  same ref from racing each other onto one draft release.

## 0.3.1 - 2026-09-03

### Changed

- Expand replay-library search to server names, base game modes, and every playable
  faction represented by the participants. Space-separated terms now match across
  different summary fields, enabling queries such as `Seaside USSR Domination`, and
  the winning faction is no longer treated as an unrelated search alias. Database
  schema 12 caches the new metadata and marks existing summaries for a normal rescan.

## 0.3.0 - 2026-09-03

### Added

- Add explicit portable, private-evidence, and coverage gates. Private Rust tests
  are now reported as ignored in portable runs and fail closed when selected;
  tracked relative fixture roles drive the 17-fixture ground truth, known Quarry
  playback control, real-install checks, and path-free 2,880-file corpus baseline.
- Add Vue Test Utils and happy-dom coverage for replay-management semantics, a
  testable import/detail controller, typed replay factories, and a table-driven
  contract for every Tauri command name and camel-case payload. Add reusable PR,
  `main`, and release quality automation with enforced V8 and LLVM coverage floors.
- Add synthetic playback Event-chain coverage with injectable resource limits and
  a runtime-independent Tauri command core covering operation exclusion, canonical
  paths, concurrent detail-load coalescing, panic recovery, and retry. Keep the
  full importer traversal optional and bound it to a deterministic 32-replay sample;
  keep the complete corpus aggregate audit as its own occasional explicit gate.

### Fixed

- Launch Tauri through its JavaScript CLI with the current Node executable so
  native Windows builds do not fail while spawning the deprecated `.cmd` shim.
- Make the release-preparation self-test use its own pending changelog entry so
  it remains valid after a release has moved all live entries out of Unreleased.
- Make the parser CLI use the library parser instead of compiling `parser.rs` a
  second time, so its 99 parser tests execute once. Correct the dormant private
  viewer assertion from historical timeline schema 7 to the live schema 18 contract,
  and deduplicate overlapping directory, glob, and explicit CLI inputs.

### Changed

- Consolidate portable CI into one locked npm install, one frontend coverage run,
  and one LLVM-instrumented Rust workspace run while retaining every test and
  per-crate coverage floor. Cache Rust dependencies and reusable build outputs for
  later quality and native release jobs, and reuse content-addressed ESLint and
  Prettier results plus TypeScript incremental metadata without narrowing any check.
- License the unified viewer and parser under the MIT License with retained
  Dex4Sure attribution. Generate locked third-party dependency notices, reject
  unreviewed licence drift in the full quality gate, verify Cargo archives by
  checksum, retain commit-pinned upstream notices for incomplete crate archives,
  and include both the project licence and third-party notices in packaged
  applications.
- Identify WiCGate as the application developer in AppStream metadata using the
  stable `org.wicgate` developer ID.
- Place Chat immediately after Overview in the replay-detail tabs, keeping the
  general Replay view before the more specialized Tactical Aid analysis.
- Pin local development and native CI/release builds to Rust 1.98.0, including
  rustfmt, Clippy, and LLVM coverage tools, so strict local checks and hosted
  quality gates cannot silently use different Clippy lint sets.
- Give replay-folder headings a restrained steel-blue surface and dark horizontal
  edges so their collapsible groups remain distinct from the library background.
  Present batch `Export selected` and `Clear` controls with the same framed secondary
  treatment as replay-name actions, while keeping counts aligned and unbroken at the
  minimum sidebar width.
- Give the active replay-detail tab a restrained dark top-and-side edge so its
  steel-blue surface remains distinct from the surrounding report background while
  preserving the established orange underline.
- Give batch-selected and open replay cards a stronger persistent steel-blue surface
  than the quieter hover state. Preserve the red-versus-blue library identity through
  a red-filled batch checkmark and the open replay's narrow red accent instead of a
  full red selection background.
- Use the same restrained steel-blue hover and pressed treatment as `Back to library`
  across neutral action buttons, while preserving the existing red treatment for
  primary and destructive actions.
- Upgrade the frontend compiler from TypeScript 5.9 to the 6.0 transition release,
  retaining compatibility with the type-aware ESLint and Vue checking toolchain
  while preparing the project for the future native TypeScript 7 migration.
- Require braces around every JavaScript and TypeScript control-flow body. Apply the
  policy uniformly to existing one-line guards so future automated edits cannot
  accidentally place a new statement outside the intended condition.
- Match Tauri's development URL to Vite's fixed IPv4 listener and cover the endpoint
  contract with a packaging test, avoiding an ambiguous `localhost` resolution path
  during `npm run tauri dev` startup.
- Start the native window and webview with the application's dark canvas color so a
  bright white frame does not flash before the frontend finishes rendering.

- Replace the full-width application masthead with a compact
  Wicgate lockup in the replay-library sidebar, preserving the product identity
  and red-to-orange top accent while giving the library and replay detail the
  full workspace height. Remove the redundant `Local archive`, `Replay library`,
  divider, and replay-count labels from that lockup so the surrounding library
  controls carry the navigation context without competing with the brand.
- Make completed library scans remove cached rows for replay files that were moved
  or deleted beneath the scanned roots, while preserving the cache when discovery
  fails or a scan is cancelled. Report the removed paths to the live frontend so
  ghost rows disappear without restarting. Library health counters now classify
  current summaries, cached parser failures, and stale rows separately instead of
  counting a cached failure as a successful parse or leaving per-scan failure totals
  to imply the state of the whole library. Scan completion preserves its camel-case
  frontend event contract, tolerates older completion payloads, and reconciles from
  the backend if Cancel discovers that the scan already ended.
- Establish a semantic UI palette while retaining the dark World in Conflict
  character: red now identifies primary actions and the current replay, orange
  identifies active navigation and utility accents, gold is reserved for keyboard
  focus, and neutral steel handles ordinary selection and interaction. Status and
  faction colors have separate roles, all interactive control families now include
  coherent hover and pressed states without layout-shifting motion, and automated
  contrast contracts protect the palette's text, focus, and border legibility. Remove
  redundant native hover tooltips from replay cards, event data, Tactical Aid, and
  leader summaries while retaining path recovery, role-icon help, and accessibility
  labels. Replace Replay's native speed selector with a compact custom menu so
  pointer selection cannot inherit WebKitGTK's persistent gold focus treatment and
  the playback row retains its original narrow footprint. The menu closes on
  selection, outside interaction, Escape, or focus leaving the control; its neutral
  hover and selected surface with an orange underline match the detail tabs, while
  the standalone trigger matches the Play button and keyboard navigation and focus
  remain explicit. Replace the decorative replay-library empty-state radar with a
  restrained replay-file icon and clearer selection guidance. Adopt individually
  imported Lucide SVG icons for that state and the Replay view's compact Play/Pause
  control without adding native hover tooltips or bundling an entire icon set.
- Let local Flatpak builds disable `rofiles-fuse` automatically when `/dev/fuse`
  is unavailable, while keeping the normal builder path unchanged on a host that
  provides it.
- Refresh the pinned Vue, Vite, Vitest, Tauri, Serde, compression, and test
  dependencies to their current maintenance releases. Build the Flatpak with the
  GNOME 50 runtime and Node.js 24 LTS SDK extension.
- Rework Tactical Aid into the Replay view's map-left/event-stream-right composition.
  The right rail now combines compact summary facts, category highlight controls, and
  virtualized deployment cards while preserving bidirectional map selection. Selected
  rows use only the existing blue background, and evidenced player/team names reuse
  Replay's USA/NATO blue and USSR red text treatment. Responsive behavior follows the
  report container: genuinely narrow panels stack the stream below the map. Detail
  tabs now divide their own width evenly, keep `Tactical Aid` on one line, and hide
  only numeric tab counts when the report is too narrow to fit them cleanly.
- Simplify replay-library counts to unboxed neutral text and make Chat channel labels
  neutral instead of treating ordinary metadata as a yellow accent. Replay cards keep
  their red selected state, but the accent before each map thumbnail now matches the
  normal one-pixel card border; the thicker right-side strip remains reserved for the
  recorder's faction color.
- Keep live map playback readable by hiding the individually serialized infantry
  members while retaining each complete squad-parent marker and the existing
  faction-colored unit-marker presentation.
- Clarify the Overview hierarchy with `MATCH ROSTER` / `PLAYERS & TEAMS` and
  `PLAYER PERFORMANCE` / `Match leaders` section labels.
- Match the original game's end-screen tie handling by ordering all 16 score slots
  with its deterministic score-only routine. Overall ranks now contain one player
  per position, and role and score-category leaders show the single player WiC
  selects instead of listing every tied player.
- Apply the established orange recorder-name treatment to the `Cost POV` summary
  value. Chat stays neutral to avoid repetitive emphasis, while Replay event-stream
  and Tactical Aid deployment names retain their more informative faction coloring.
- Preserve the original replay filename in the single-file export save dialog
  instead of unnecessarily adding a `-copy` suffix. Existing destination files and
  the source path itself remain protected from overwrite.
- Reopen single- and multi-replay export dialogs at the last successfully used export
  directory, or Documents before one has been remembered, while retaining the current
  replay file name for single-file exports.

### Added

- Add a compact `Back to library` action to replay detail views, with Escape-key
  navigation, removal of the opened replay from batch selection, and focus restoration
  to its library card while preserving the library's search, grouping, and scroll state.

- Add pinned, type-aware ESLint checks for the Vue and TypeScript frontend plus the
  JavaScript tooling. Enforce Vue's essential correctness rules and a small set of
  deterministic conventions while leaving all code formatting to Prettier, and run
  linting during the fast pre-commit quality gate.

- Add pinned Prettier formatting for the frontend, JavaScript tooling, configuration,
  workflows, and documentation. Provide explicit write and check commands, exclude
  dependencies, build output, lockfiles, generated assets, and synchronized agent
  instructions, and enforce formatting during the fast quality gate.

- Publish native Linux DEB and RPM packages plus ad-hoc-signed Apple Silicon and
  Intel macOS DMGs alongside the Flatpak and Windows portable ZIP for tagged
  releases. The release workflow builds each package from the synchronized product
  revision on a native platform runner, stages stable architecture-specific asset
  names, and includes all six files in the draft-release asset verification. The
  macOS builds require no Apple credentials but remain unnotarized.

- Add replay management for separately displayed and searchable file names and
  in-game `ReplayName` metadata. Renaming changes the original file name in its
  current folder without overwriting an occupied destination; its field hides and
  automatically retains the `.wicdemo` extension. Export
  retains the source and creates an exact copy, either individually or in selected
  groups through an all-or-nothing folder export with an optional new target folder
  created by the app. Replay cards use conventional
  click, Ctrl-click, Shift-click, and Ctrl-Shift-click selection with an integrated
  map-tile checkmark instead of a separate checkbox gutter. Export lives in the
  library selection toolbar for both one and several replays, while the detail
  header retains only single-replay rename actions. In-game names remain a separate
  single-replay edit. Direct name edits rebuild the framed
  compressed stream through a verified temporary file and rollback copy; parser,
  writer, database-migration, collision, corruption, Unicode, batch-export, and
  private real-replay semantic-equivalence regressions cover the path. Library rows
  reserve the in-game name's full line box and matching virtual-row height so the
  added field remains vertically unclipped.

- Add release-time semantic-version preparation with one npm command for patch,
  minor, or major releases. `package.json` is the product-version source, Tauri
  reads it directly, and the command synchronizes Cargo, lockfile, AppStream, and
  changelog copies. Product metadata is checked during normal quality gates and
  against `v*` tags before artifact builds; valid tags create a draft GitHub Release
  containing the Flatpak and Windows portable ZIP. The embedded parser crate is now
  explicitly unpublished and its local dependency no longer couples the unified
  product to the parser's historical `6.0.0` crate version.

- Import the complete standalone replay-parser history beneath `parser/` and make
  its Rust crate a local workspace member. Parser and viewer contract changes can
  now land atomically without a nested private submodule; the original-to-rewritten
  commit map preserves traceability to the archived parser repository.

- Add a replay-time domination bar above map playback for Domination and Tug of
  War. It follows the scrubber using the parser's recording-time samples, holds
  the last recorded value rather than interpolating, and remains unavailable when
  the recording cannot attribute the curve to concrete factions. The shared meter
  keeps the Match Summary precision, faction ordering, and colors. The synchronized
  event stream is now a bounded companion panel aligned beside the bar and map,
  including at the stacked responsive breakpoint, without overflowing the replay
  viewport.

### Changed

- Rename the user-facing product from `Wicgate Replay Viewer` to `WiC Replay
Viewer` across the application header, window title, desktop and AppStream
  metadata, release title, documentation, and distribution artifact names. Keep
  the stable `org.wicgate.ReplayViewer` application identity, `wic-replay-viewer`
  executable, repository, Cargo packages, and application data paths unchanged.

### Fixed

- Use the same neutral platform scrollbar treatment as Overview throughout the
  replay library, saved locations, Chat, Replay, and Tactical Aid views. Remove
  the conflicting blue thumb and WebKit track overrides that produced an
  inconsistent pressed state while retaining the library card clearance gutter.
  Reduce the library resize seam to a narrow, uniformly colored draggable gap
  without mismatched blue borders or a permanent grab handle.
- Bound hostile replay and installed-map inputs before first release. Replay files
  must use the `.wicdemo` extension and are size-checked before allocation. Parsing
  and playback share compressed, decompression, structural-work, and process-concurrency
  budgets; imports use every detected logical processor without oversubscribing that
  capacity, and projectile, death, and Tactical Aid ownership joins use bounded indexed work.
  Player identity and faction lookups use sorted per-slot searches, and live
  attribution evidence has its own construction-time event cap. Countdown-phase
  anchoring and recorder-frame changes are processed in single passes, while replay
  discovery caps directories, filesystem entries, files, and aggregate path bytes.
  SDF blocks reject oversized stored lengths before reads, stop zlib expansion at
  the declared output size (including cumulative codec-3 output), and cap retained
  entry-path storage; map-art scans account cross-archive retained entries and paths
  before storing another opened archive and cap cumulative decoded source work as
  well as retained PNG output.

- Require the complete quality gate before either release artifact can build. The
  release workflow now pins every third-party action to a full commit, passes tag
  names through the environment instead of shell interpolation, rejects tags not
  reachable from `main`, rechecks that the remote tag still targets the built commit
  before release mutation, and removes or rejects stale assets in the draft release.

- Keep Replay event-stream rows neutral and color only facts already named in
  their descriptions. Every exact player name is colored from that player's team
  assignment at the event time, while literal `USA`/`NATO` and `USSR` labels use
  the same blue and red faction colors. Events naming both sides color each name
  independently; unknown players and unresolved teams remain neutral. The obsolete
  row-level actor-faction projection is removed, so presentation does not infer a
  killer, cause, identity, or event that the replay did not provide. Existing
  cached details are refreshed for the per-event player/faction display data.

- Show the replay-time domination bar for spectator recordings when the parser's
  winner-inferred final shares deterministically identify which faction the raw
  sample curve represents. Playback still holds serialized samples without
  interpolation and stays hidden when the anchor or opponent is ambiguous. Assault
  remains excluded because its factor is attacker progress, not two-sided control.

- Correct the parser README's timeline schema and ground-truth fixture counts to
  match schema 18 and the 17-fixture manifest.

- Correct the application reverse-DNS identity from `com.wicgate` to the owned
  `org.wicgate` namespace across Tauri and Flatpak metadata.

- Keep the established community map-name mapping authoritative, correct its
  `as_AirBase` spelling, and add the missing verified `tw_Radar`, `tw_Highway`,
  and `tw_Wasteland` names. Airport and Wake revision paths now reuse `do_Airport`
  and `do_Wake` instead of exposing invented numbered public names, and the base
  Wake path gains its missing Domination prefix. Black Forest and Helgoland retain
  their established compatibility labels. Unknown paths remain raw.

- Correct Overview player names and roles in older replays where one player leaves
  a numeric slot and another enters it before gameplay. The viewer now pins the
  parser's match-start scored-roster resolution and advances the detail cache key so
  existing cached rows are rebuilt. A private positive control covers the reported
  Seaside replay and requires distinct Infantry and Air players.

- Name command-point captures in the Replay event stream from each map's installed
  localized `CommandPoint__*` UI labels, matched to the exact object ID serialized
  in the replay. Missing or malformed map labels keep the generic command-point
  wording, and no game localization data is bundled with the viewer.

- Add compact post-match leader lists below the player results for command points,
  fortifications, repairs, enemy damage, and Tactical Aid. Values come directly from
  the replay's serialized final
  player summaries; zero-only categories are hidden.
  The same section also shows the top three end-screen totals and the best
  Infantry, Support, Armor, and Air scores.
  Rankings use compact alternating rows without redundant score descriptions, and
  the replay recorder keeps the orange name treatment used in the team cards. Role
  leaders reuse the team-card role icons with matching lowercase accessible labels
  and tooltips. The three leader groups stay vertically stacked until the report
  itself has enough width to present all three columns without truncating player
  names.

- Correct command-point, unit, and Tactical Aid placement by projecting against the
  full terrain represented by `overviewmap.dds`, rather than stretching the playable
  ICE rectangle across the image. Bounds are derived deterministically from each
  map's square 16-bit heightmap metadata, and overview and heightmap entries now obey
  archive precedence independently. The map-art cache key refreshes existing rows
  without a database migration.

- Cap active replay-map redraws at 30 frames per second while advancing from real
  elapsed time. Playback no longer keeps an uncapped animation loop alive, and its
  timer is cleared on pause, replay replacement, completion, and unmount.

- Add replay-defined command-point capture circles to the Tactical Aid map as a
  neutral geographic reference layer beneath deployment markers. The tab uses a
  lightweight objective-only backend response instead of loading unit frames.

- Render command points as their replay-defined two-to-four perimeter capture
  circles instead of a single square at the parent anchor. Each circle follows
  its own `SetPerimeterPointOwner` history and remains grouped by the serialized
  parent command-point ID.

- Keep synchronized event-stream cards at the virtualizer's exact 58-pixel row
  height and ellipsize long labels/descriptions, preventing wrapped content from
  painting over the following event.

- Add a lazy Replay watch view that reconstructs the selected match on its verified
  map beside the synchronized chronological event stream, replacing the redundant
  standalone Timeline tab. The stream reveals only events at or before the current
  replay time, hides later events again when rewinding, and follows the newest
  revealed row without a redundant current-event marker. Selecting a revealed event
  seeks the map.
  Play/pause, scrubbing, and speed controls drive authoritative full/compact unit
  checkpoints, unit lifecycle removal, command-point ownership, and positioned
  Tactical Aid pulses. The high-volume stream is decoded only when opened and
  explicitly holds the last checkpoint between recorded frames instead of
  presenting interpolation as replay truth.

- Plot every position-bearing Tactical Aid deployment over the user's own map art.
  Markers use the event's evidenced faction (red USSR, blue USA/NATO), preserve exact
  coordinates, group only identical x/z positions, and open deployment details on
  selection. Map markers and Tactical Aid timeline rows share bidirectional selection:
  either one highlights the other, and markers scroll the virtualized timeline to the
  matching event. The redundant marker detail menu is removed in favor of that single
  source of detail, and each per-faction support counter can highlight all map locations
  in its category. Category matches retain their faction-colored marker and use the same
  outer white selection ring as individual selection, without an extra marker border.
  Projection follows the overview image's shipped 180-degree orientation,
  rather than displaying world x/z in the opposite direction. Coordinates outside
  verified terrain bounds remain in the table and are reported rather than clamped
  or guessed.
- Cache verified map coordinate bounds with each decoded overview image. Schema 8
  preserves existing PNG rows during migration, while the independent map-art cache
  key refreshes bounds without invalidating replay parser output.

- Make cached map art appear immediately and reliably on application startup across
  Linux, Windows, and macOS. The initial library response now includes the existing
  art count, availability is reactive for tiles that mounted before validation, and
  visible unique maps share one bounded Tauri/SQLite batch instead of opening and
  initializing the database per image. Transient command failures receive two bounded
  retries without being cached as permanent misses, and cache generations prevent an
  old in-flight response from repopulating a changed source.

- Preserve the last complete map-art cache when a configured game or downloaded-map
  folder is temporarily unavailable. Background validation reports the unreachable
  source without rebuilding from an incomplete subset, and a manual rescan now
  invalidates only the signature so existing images remain usable until a replacement
  transaction commits successfully.

- Stop presenting the invalid team-zero result sentinel as a spectator victory in
  the timeline. Incomplete matches now state that the match ended without a valid
  winner, while genuine USA, NATO, and USSR victory events remain unchanged.

- Reduce native replay-library scan overhead on Linux, Windows, and macOS without
  changing parser output or the Tauri event contract. Discovery now compares every
  replay fingerprint against one SQLite cache snapshot, and the coordinator commits
  summaries in atomic groups of at most 64 instead of starting one transaction per
  replay. Stored-summary events are still emitted after successful commits, and an
  exceptional batch failure retries each row separately so one bad update cannot
  suppress unrelated summaries. Imports continue to use every detected processor
  thread by default; an eight-worker experiment was rejected because it made the
  1,055-replay corpus scan 9.2% slower.

- Remove the typed-parser-to-JSON-to-display-model round trip on a replay's first
  detail load. `DetailView` is now projected directly from the typed parser document
  while the same raw parser JSON remains the lazy SQLite cache and evidence layer.
  A 16-replay distributed corpus parity test requires the direct and legacy cache
  projections to serialize identically. Tactical-aid effect keys are also traversed
  deterministically, preventing equal-time rows from changing order with randomized
  process hash seeds.

- Coalesce simultaneous native detail requests for the same replay path. One
  background parse/cache load now serves every waiter; completed and failed keys are
  removed so later selections can use the cache or retry normally.

- Bound every development-time discovery layer. Vite's dependency optimizer starts
  from `index.html` instead of globbing the repository for HTML; its watcher ignores
  Cargo `target/` trees and refuses to follow symlinks; `.taurignore` keeps Tauri's
  independent Rust watcher out of build and package trees; and Tailwind scans only
  `frontend/`. A Wine validation prefix's `dosdevices/z:` host-root mapping can no
  longer make these tools traverse `/proc`, consume several gigabytes, crash with
  `EINVAL`, trigger an SELinux denial, or leave the Tauri window white while loading.

- Move replay-folder actions into the library panel and separate adding a
  location from scanning it. Adding folders now saves them without parsing;
  the panel then offers `Scan new folder`, `Refresh library`, or the quieter
  `Scan all folders` action according to the current state.
- Add a cooperative `Cancel scan` action that safely keeps completed replay
  summaries while stopping further discovery and parsing.
- Disable Flatpak build-cache lookups so local directory sources are always staged
  from the current checkout instead of reusing an older application tree.

- Colour the after-action victory badge by the winning faction: NATO and USA use
  blue, USSR uses red, and both use white text with a restrained orange winner
  edge matching the winning team cards.

- Raise the resizable replay library range from `360–680px` to `460–720px`
  and use `520px` by default. The new minimum keeps the complete row metadata
  visible, while the default balances replay titles against the detail view.

- Group the replay library by each replay's actual parent directory. Recursively
  discovered subfolders now appear as independently collapsible folder rows with
  replay counts, so one configured archive can still be managed folder by folder.

- Separate team-size formats such as `4vs4` from Server mode and display Format
  immediately beside Server mode in the replay library and overview.

- Keep the active detail tab red while the pointer is over it, for the same reason
  and by the same means as the selected library card below: hover invites a switch
  and has nothing to offer the tab already showing, but `.detail-tabs button:hover`
  outranks `.detail-tab-active` on specificity and had been washing it out. Inactive
  tabs are unaffected.

- Keep the selected library card red while the pointer is over it. The hover tint
  is an invitation to select, and a replay that is already selected cannot be
  selected again, so hover no longer recolours it; `.replay-card:hover` outranks
  `.replay-card-active` on specificity and had been winning regardless of source
  order. Every other card, including one that failed to parse, keeps its full hover
  treatment, and a selected failed replay keeps the orange border because the
  failure is shown nowhere else on the card.

- Offer the add button in the library locations card even when no folders are
  configured. The button was hidden until a location existed, so the empty state
  could only point at the header, and the card now matches the map art card below
  it: a hint line with its action directly beneath. The label reads "Add replay
  folder" while empty and "Add another location" afterwards.

- Source an untracked `.env` from `scripts/quality.sh`, so the tests gated on a
  real installation or a private replay corpus run by default wherever those paths
  are recorded. Cargo reports a skipped test as `ok`, so those suites previously
  printed a passing line whether or not they had read anything. A fresh clone has
  no `.env` and skips them as before. See README, "Environment-gated tests".

- Draw the selected match's own overview art behind its after-action report. The
  image comes from the cache the library tiles already fill, so the backdrop costs
  no extra decoding and appears only where art exists; a map without art, or a
  viewer with no installation configured, renders the report on the plain panel as
  before. Readability comes first: the art is blurred, desaturated, and dimmed under
  a downward-darkening scrim, the report's cards stay opaque enough to read against,
  and `prefers-contrast: more` drops the backdrop entirely. One shared surface now
  carries it across Overview, Timeline, Chat, and Tactical Aid; the dense table tabs
  use darker translucent rows and headers to retain legibility.

- Read map art from two independent sources. The game folder supplies the shipped
  maps; community maps live at `Documents/World in Conflict/Downloaded/maps`
  wherever the game itself is installed, and ship as ordinary `.sdf` archives named
  after the map rather than as `wic<n>.sdf`. Shipped archives are read first and
  downloaded maps second, so a community map overrides a shipped map of the same
  name.

- Detect both folders on first run instead of demanding they be chosen. Standard
  Steam, GOG, and Ubisoft install locations are checked, along with Wine prefixes
  and Proton's per-title `compatdata` prefixes, preferring the `steamuser` profile
  and searching the prefix that holds the installation first. Three setting states
  are distinguished: unset means never configured and detection runs, empty means
  the user cleared it deliberately and detection stays out of the way, and anything
  else is their chosen path. A detection miss stays unset so a later installation is
  still found. Both pickers remain available at all times, and a rescan re-reads the
  archives after a game is patched in place under an unchanged path.

- Offer the game's default replay folder once, on a library that has no locations.
  Adding a location imports nothing by itself, and the seeding is recorded so
  removing it sticks.

- Fix map art never resolving. Replay summaries store the map as an archive path
  such as `maps/ustown4/ustown4.ice`, not as the bare `ustown4` the art is keyed by,
  so every lookup missed and every row silently kept its procedural tile. Summary
  map fields are now reduced to the internal name, handling both separators.

- Draw each replay row's real map overview, read at runtime from the user's own
  game installation. A new bounded SDF archive reader decodes the shipped texture
  codec: a 128-byte prefix held in the archive's version-10 auxiliary metadata
  followed by three independently inflated zlib streams. Overview art is decoded
  once per installation into its own SQLite table, then served per map name, so a
  virtualized list of thousands of rows never decodes per row and never blocks
  scrolling on an archive read. No game art is bundled or redistributed; the
  installation is only ever read, and having none is a normal state that simply
  leaves every row on its procedural tile. Where a map ships in more than one
  archive the higher-numbered one wins, which is where the revised art lives.
  Map art has its own cache key and never invalidates cached replay detail.

- Declare `color-scheme: dark` so browser-level forced-dark cannot re-map the
  palette and wash over the map tiles.

- Draw a procedural map tile on each replay-library row. The artwork is derived
  from the map name with a stable 32-bit hash, so a map looks the same on every
  launch and machine, and hues are drawn from a curated ring rather than the full
  circle to stay inside the existing palette. The tile distinguishes maps; it does
  not depict them, and it is explicitly the fallback layer for the game's own
  overview art. An unresolved map still renders as a neutral tile rather than
  collapsing the row layout. The recorder's faction moves onto the tile's inner
  edge instead of the accent stripe, because selection already owns red on the
  card border, background, and accent glow.

- Consume parser schema v18 destruction contexts. Exact occupied-building collapse
  and destroyed-with-container losses now use explicit mechanical prose while
  remaining faction-neutral if the initiating attacker is unknown. A container
  child names tactical aid only when the parser independently proves and inherits
  its parent container's exact support/team; no contextual row infers a player.

- Present genuinely unresolved destruction rows as `UNIT LOST` with direct,
  neutral loss prose such as `Alpha lost a unit`. Proven player and tactical-aid
  causes remain `UNIT DESTROYED`; parser evidence and cache semantics are unchanged.

- Consume parser schema v17 destruction causes. Exact-target tactical-aid deaths
  name the serialized issuing team and aid, while genuinely unresolved deaths use
  neutral unit-loss prose instead of repeating `cause unknown`. Raw cached parser
  evidence remains unchanged and no player is inferred for a team-only TA kill.

- Pin canonical parser schema v16 and rebuild cached replay details. Individual
  infantry actors now remain in the parser's separate `infantrySoldierDeaths`
  collection, while the viewer timeline consumes only complete unit and squad-parent
  `unitDestroyed` events.

- Rename impossible tactical-aid faction fallbacks to `Invalid faction data` and
  suppress those rows from both Timeline and Tactical Aid presentation. The
  canonical parser already rejects deployment teams outside USA/NATO/USSR; this
  guards only malformed, stale, or future cached parser documents. Advance the
  detail cache key so previously projected rows are rebuilt.

- Count zero-score players in a clan match's team sizes. The matchup suffix drops
  zero-score slots because a zero score normally marks someone who never played,
  but a clan match is a pre-arranged roster with no spectators and no mid-joiners,
  and a player who disconnects just before or exactly at the final whistle is still
  recorded with a zero score. Excluding those reported a real 5vs5 as `4vs5`.

  Across the 931 clan replays in the corpus this changes 94 matchups. 53 asymmetric
  results become symmetric and 26 that could not resolve at all now do — those had
  one side emptied entirely by the exclusion, so no suffix was shown. Against that,
  11 become asymmetric because the zero-score player really was an extra body. A
  score of zero cannot distinguish the two cases, so this is a deliberate trade of
  11 newly wrong for 79 newly right rather than a rule that is always correct.
  Narrowing the remaining 11 needs the player's `participantSession` end time to
  separate a late leaver from someone present throughout, which the matchup helper
  does not currently receive.

  Every other mode keeps the exclusion, and the spectator slot is still excluded
  everywhere. Cached summaries are rebuilt once on next launch.

- Put pre- and post-match chat on the recording clock. The Chat tab's time column
  read `-0:12.3` and `+0:05.0` — offsets from the match boundary, not positions in
  the replay — so a chat message could not be lined up against a Timeline or
  Tactical Aid row. All three stages now read the recording axis, and the Stage
  column still says which side of the match a message falls on. This reads the
  parser's new `timeSeconds` on both chat boundaries (timeline schema v14) rather
  than reconstructing the match boundary in the viewer, which would have re-derived
  a fact the parser already had.

- Read Timeline, Chat, and Tactical Aid timestamps as `M:SS.s` rather than raw
  seconds. Timestamps run to the length of a recording, where a bare `812.4s` stops
  being readable well before the end of a round. Tenths are kept because the events
  resolve to them and simultaneous ones must stay distinguishable, and the chat
  labels keep their `-`/`+` prefixes for pre- and post-match messages.

- Name the side as the actor on a tactical-aid row whose exact player was never
  proven: `Team USA` rather than `Unknown player`. Opposing-faction strikes cannot
  be attributed to a person — only a player-bearing marker or a validated
  unit-drop owner proves that — but the recording does prove which side fired
  them, so the old wording read as a gap in the data when the row in fact carries
  a definite fact. It is phrased as a collective so it cannot be mistaken for a
  player named `USA`, the `Team only` evidence label is unchanged, and a side the
  recording never resolved to a faction still reads `Unknown player`. The matching
  timeline row now leads with the same actor.

- Rename the Tactical Aid table's `Support` column to `TA used`. Support is also
  one of the four game roles, so the old header read as a role filter rather than
  as the strike each row records.

- Stop a broken remote object from aborting the Flatpak build. `--install-deps-from`
  makes flatpak-builder update the runtimes it already has rather than only
  installing the ones it lacks, and no flag separates the two, so a bad object on
  Flathub failed a tree that builds fine — arriving as a libostree SIGSEGV rather
  than a clean error, with both the packaged flatpak-builder and the
  org.flatpak.Builder app. `build-flatpak.sh` now reads the runtime, SDK, and SDK
  extensions from the manifest, checks each one, and asks for dependencies only
  when something is genuinely missing. A fresh clone still installs them on the
  first run.

- Advance the pinned parser to the commit that closes the Ranked recovery
  question. Documentation only, so parser output is unchanged and the cache key
  moves solely because it names the pinned commit; cached summaries are rebuilt
  once on next launch.

- Label the residual server mode `Ranked`: a replay with FPM, Match Mode, bots,
  clan match, and tournament match all positively ruled out is a ranked public
  game. The demo header does not serialize the dedicated server's `RankedFlag`, so
  the parser still reports `serverClassification.ranked` as null and this label is
  an inference made at the display layer, closed by operator ground truth that
  unranked public servers were not run in practice. It is one-directional: the
  dedicated server permits Ranked alongside Match Mode, clan, and tournament
  servers, so the other labels do not mean unranked. A classification missing any
  of the five inputs stays unlabelled, because absent evidence is not proof a mode
  was absent. Parser output and the cache key are unchanged.

- Advance the pinned parser to pick up the exact serialized message names recorded
  alongside its hash constants. Comments only; parser output is unchanged, and the
  cache key moves solely because it names the pinned commit.

- Show replay length rather than match length in the library list, and lead the
  Overview facts with it. The two are different questions: a replay can hold ten
  minutes of lobby before the match starts, and a mid-join recording is shorter
  than the match it observed. Match length remains in the Overview and on the
  library row's tooltip.
- Fix the Overview's "Recording length" fact, which read
  `timing.recordedSeconds` — a countdown-derived value that excluded lobby and
  post-match time and understated the file by a corpus median of 76 s. It now
  reads the parser's new `timing.recordingSeconds`.
- Advance the pinned parser to timeline schema v13, where every timestamp is
  recording-elapsed time from the `Event` envelope clock rather than an
  interpolation between countdown samples. Timeline positions shift for every
  replay, so the parser cache key is bumped and cached details are rebuilt. The
  Timeline tab's "Mission clock" is relabelled "Replay length" to match what the
  value now is.
- Add a `recording_seconds` column to the replay summary table (database schema
  v5) so the library can sort and display replay length without loading details.

- Add `npm run build:windows-cross` for producing the portable Windows executable
  from Linux with `cargo xwin`. Beyond `cargo-xwin` and the Rust target it needs
  `clang-cl`, `llvm-lib`, `llvm-rc`, and `lld-link`, of which only the first is a
  usual Linux development dependency; the script names whichever is missing
  instead of failing mid-build. The native Windows job stays the release path
  because a cross-linked executable is not bit-identical to it.
- Stop rounding the final control percentages to whole numbers. The bar resolves
  to 1/3000, so rounding turned a real result such as USA 50.07% / USSR 49.93%
  into a false 50/50 tie. Labels now carry two decimals with trailing zeros
  trimmed, and segment widths use the unrounded values.
- Drive the control meter from the parser's faction-attributed shares rather than
  from winner/loser, and title it for the mode: a domination bar for Domination, a
  front line for Tug of War. Assault stays excluded.
- Show match length and recording length separately, report how the match ended
  (total domination, timer, or forfeit), and note when the recorder joined
  mid-match along with the clock time they joined at.
- Center each faction name and final control percentage together within its side
  of the domination bar for a clearer tug-of-war presentation.
- Carry directly evidenced factions into timeline rows and use the Overview's
  battlefield palette for Timeline events and Tactical Aid factions: red for USSR
  and blue for USA/NATO. Color unit-destruction events by the directly recorded
  killer team while leaving unknown-cause destructions neutral.
- Append resolved team sizes to server-mode labels, such as `Clan Match 4vs4`,
  while excluding spectators and zero-score players, and retaining the existing
  label when player team assignments are incomplete or do not resolve to exactly
  two teams.
- Prevent large stale-library rescans from freezing the WebKit UI. Buffer stored
  replay summaries outside Vue's reactive state while parsing, keep progress
  counters live, then merge by path and sort the virtualized library once when the
  import finishes.
- Display independent replay-backed server-mode labels for FPM, Match Mode, bots,
  clan matches, and tournament matches in the library and overview. Keep Ranked
  hidden while the parser reports it as unknown, migrate cached summaries to
  schema v4, and refresh them through the new parser cache key.
- Place the server mode beside the replay name in the library using regular text
  color. Prefer the specific Clan Match, Tournament Match, or Bots label over a
  redundant Match Mode prefix, while retaining the observed `FPM · Bots` outlier.
- Widen the replay library by default, reserve an explicit rail so its overlay
  scrollbar cannot cover replay cards, and make the library/detail divider mouse-
  and keyboard-resizable with a persisted width and a protected minimum detail width.
- Show the recorder's faction and name in replay-library badges instead of the match
  winner, with an in-place schema-v3 summary migration and cache refresh for existing
  rows. Preserve the recorder's exact Unicode casing and clan-tag punctuation rather
  than applying the status badge's uppercase transformation. Order row metadata as
  recorder, map, faction, and duration, with the recorder distinguished by plain
  orange text. Remove the ambiguous player count because it mixes active players,
  spectators, and inactive participants.
- Keep USA/NATO on the left and USSR on the right in team cards and the final
  battlefield-control meter, regardless of which faction won, and label the cards
  explicitly as `WINNER` and `LOSER`.
- Soften the midpoint divider in the domination/control bar to a muted divider style
  for a less intrusive visual emphasis.
- Mark release builds as Windows GUI applications so the portable executable does
  not open an extra console window when launched.
- Fix the blank Flatpak window. The manifest built the app with a plain
  `cargo build --release`, which left Tauri's `custom-protocol` feature disabled, so
  `generate_context!` emitted a development context with no embedded frontend and the
  webview loaded the unreachable `devUrl`. Declare the feature on the app crate,
  enable it in the Flatpak build command, and guard both in the packaging tests.
- Restore native Linux development and DEB/RPM packaging alongside Flatpak. Ignore
  Flatpak builder state in Vite's development watcher so symlink loops in the build
  root cannot break `npm run tauri dev`.
- Make Flatpak the primary Linux distribution artifact with GNOME 49 runtime
  metadata, the verified X11/GPU path, read-only replay access, no runtime network
  permission, and a reproducible local bundle script. Disable Tauri's native Linux
  bundling so DEB, RPM, and AppImage packages are no longer produced or supported.
- Add a portable Windows x64 standalone-executable ZIP and disable native Windows
  installer bundling so NSIS and MSI packages are neither produced nor supported.
  Build the portable ZIP and Flatpak through a manual/tag-triggered artifact
  workflow. Keep macOS available as secondary local `.app` and `.dmg` builds pending
  Developer ID signing and notarization.
- Validate Flatpak identity, sandbox permissions, desktop/AppStream metadata,
  platform bundle exclusions, and workflow artifact paths in the viewer
  quality gate.
- Restrict Vitest discovery to viewer-owned frontend tests so generated Flatpak
  build trees and document-portal mounts cannot inject unrelated test suites.
- Load the public header logo as a runtime URL so a transient Vite asset-module
  miss cannot abort the root Vue module graph and leave WebKitGTK blank.
- Show the game role icons in player rosters and a viewer-native tactical control
  meter for final faction percentages in completed two-team Domination and Tug of
  War match overviews; omit the meter from phase-based Assault matches and omit a
  defeated faction's segment at 0% control.
- Resolve every timeline, destruction, chat, and tactical-aid player slot against
  schema-v11 time-bounded participant sessions. Preserve the established static
  participant fallback for slots without session evidence, but show an unknown
  identity during explicit leave/re-entry gaps instead of leaking the old occupant.
- Use the source filename stem as the replay title while retaining the full filename in Source file.
- Replace redundant game-mode UI fields with the translated map identifier, whose prefix carries the mode.
- Present unassigned players as spectators, leave unavailable player roles blank,
  and rely on accessible, hover-labeled role icons instead of repeating role names
  beneath player names.
- Render spectators as a compact neutral roster beneath the competitive team cards.
- Describe command-point owner zero as neutral instead of attributing a capture to spectators.
- Label destroyed command-point fortifications by their shipped anti-air, anti-tank, or machine-gun type, and omit destruction rows with no identifiable participant or object.
- Display schema-v10 tactical-aid damage notifications as exact actor/support/target
  timeline rows without claiming they killed a simultaneous unit; retain a generic
  tactical-aid label when the replay catalogue cannot resolve the raw index.
- Distinguish same-player destruction from missing killer attribution, replacing
  ambiguous `player lost a unit` prose with explicit self-fire or `cause unknown`.
- Force WebKitGTK's shared-memory renderer transport by default on Linux to prevent
  a fully initialized Vue/Tauri window from presenting as blank white on the
  affected Fedora DMA-BUF path. Apply the default in the npm launcher so it exists
  in Tauri's initial environment rather than relying only on a late Rust mutation;
  an explicit `WEBKIT_DMABUF_RENDERER_FORCE_SHM` value remains authoritative.
- Default the Linux viewer itself to the verified X11 GTK backend, overriding the
  desktop-wide Wayland backend that repeatedly produced a blank WebView on the
  mixed-scale monitor layout. Add a viewer-specific Wayland retest override and a
  launcher regression test so unrelated feature changes cannot silently remove the
  native rendering policy.
- Added the initial native replay library and master-detail viewer.
- Added auto-detected and manually bounded parser concurrency.
- Added SQLite summary storage, fingerprint checks, and lazy detail caching.
- Added virtualized Overview, Timeline, Chat, and Tactical Aid views.
- Replaced the minimal `eframe` presentation layer with a Tauri 2 desktop shell and a
  typed Vue 3, Vite, and Tailwind frontend.
- Ported the Wicgate visual language into viewer-owned design tokens, panels, replay
  cards, team views, status feedback, and virtualized data tables.
- Preserved the original native database location and backend concurrency/cache
  invariants behind a narrow Tauri command and event contract.
- Added locked frontend tests, strict Vue/TypeScript checking, production asset builds,
  and Wicgate-derived desktop application icons.
- Added persistent multi-folder libraries, multi-select folder browsing, native folder
  drag-and-drop, aggregate rescans, and source removal with overlap-aware replay cleanup.
- Removed the legacy worker selector and duplicate boxed-W motifs; imports now use all
  detected processors automatically and the Wicgate wordmark is the sole brand anchor.
- Added a human-readable timeline projection that resolves participant names and
  hides raw player, unit, support, vote, and team identifiers from feed prose.
- Ordered result cards as winner, opposing playable faction, spectators, then
  unassigned/unknown groups.
- Replaced heuristic server-title detection with the parser's structural
  `myGameName` field and invalidate stale cached details.
- Corrected Tactical Aid coverage: top-level delayed-spawn effects now show both
  factions in player and spectator recordings, while player names require an exact
  marker or another explicit parser attribution source.
- Added schema-v9 presentation support for the parser's validated unit-drop owner:
  opposing and spectator deployments show that resolved participant while ambiguous
  or non-unit aids remain team-only. The parser pin and cache key now invalidate
  schema-v8 replay details automatically, and the quality gate rejects future
  parser-pin or timeline-version drift from that key.
- Added faction/evidence columns, team-split summaries, spectator-view status, and
  conservative recorder-only cost enrichment while suppressing duplicate raw
  purchase/marker/effect rows.
