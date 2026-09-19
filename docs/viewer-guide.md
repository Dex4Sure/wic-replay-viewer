# Using WiC Replay Viewer

See the [README](../README.md#first-use) for initial setup. This guide describes
the development version; published releases may have fewer features.

## Library and replay views

- Index multiple replay folders recursively and browse large libraries through a
  responsive, virtualized interface.
- Search filenames, in-game replay names, servers, maps, game and server modes,
  match formats, dates, players, recorders, and participating factions. Space-separated
  terms can match different fields, so searches such as `Seaside USSR Domination`
  work as expected. Use `player:`, `map:`, and `faction:` for precise
  filters; player and faction filters match the same result occupant. Search tips
  beside the search box explain the syntax. Replays remain grouped by their
  containing folder, with current, stale, and failed cache entries tracked separately.
- Inspect teams, players, roles, match results, server mode, recording length,
  match length, final control, and end-of-match score leaders.
- Play the battle on a 2D tactical map using serialized unit checkpoints, command
  point ownership, destruction timing, Tactical Aid labels and faction-aware
  countdowns, seeking, and speed controls. Between checkpoints, the viewer holds the last recorded state rather
  than inventing movement.
- Browse synchronized Replay, Chat, and Tactical Aid event streams. Player and
  faction styling uses event-time evidence and does not infer an unknown actor,
  killer, or cause.
- Plot Tactical Aid deployments at their replay coordinates and link map markers
  bidirectionally with the deployment stream. Compact faction cards keep aid
  categories above the scrolling event list; selecting a category highlights markers
  without filtering or altering the underlying events.
- Load overview art, terrain bounds, and command-point names at runtime from your
  own game installation and downloaded maps. No game textures are bundled or
  redistributed, and procedural map tiles remain available without an installation.
- Rename replay files, export byte-identical copies, and edit the separately stored
  in-game replay name through explicit, validated management actions.
- Keep imports bounded and cancellable, cache summaries in SQLite, and parse full
  replay details only when selected.

## Match results

Overview and library summaries prefer the replay's end-game screen: its final
players, teams, and total/category scores. Those totals can differ slightly from
live playback scores or server-reported results. Departed players are absent from
supported final screens; spectators and unassigned players are listed by name only.
Incomplete or ambiguous final results retain the previous reconstruction. A readable
score table alone is insufficient: missing compressed data before the result or an
unresolved final player identity also requires fallback. The same
fallback is used when it restores an opposing side missing from the final screen,
using its roster and scores for both sides together.
Match formats count the selected result roster's team members regardless of score
or server mode. Spectators are excluded; an unknown team leaves the format unknown.
Empty teams retain their normal Overview card with no player rows. See
[how results are selected and the known fallback recordings](final-screen-results-2026-09-09.md).

## Searching

Search `generalx nato riviera` to find GeneralX playing NATO on Riviera, regardless
of who recorded the replay. Player names and factions match the same selected
result entry. Without a player name, a faction matches either team. Explicit
`player:`, `map:`, and `faction:` filters also work; quote names containing spaces
(`"Weed Queen" ussr`). Server names, formats, dates, and other metadata still combine
with these terms: `generalx nato riviera ranked 4vs4` also requires matching ranked
and 4vs4 metadata. Winner and loser status are display information only. Queries
run against prepared in-memory summaries; they never open replay files or load details. After upgrading an older library,
use **Refresh library** once to populate the new player/faction pairs. See
[search syntax, caching, and measured performance](library-search.md).

## Tactical Aid markers

Recorded explosions show brief flashes and rings at their own positions and
radii. Napalm appears as orange patches and chemical strikes as translucent green
clouds, lasting for their recorded effect lifetime. Bombing runs and barrages
follow individual recorded impacts. Generic explosions can also come from normal
combat; they are not assigned to a TA or player without that evidence.

Recorded nuclear detonations show a brief flash, expanding shockwave and fading
glow beneath the map markers. The effect follows playback time and seeking; it
appears only when the replay contains the supported nuclear effect record.
Reduced-motion settings suppress the flash and moving ring.

On the replay map, every Tactical Aid shows its name, placing player and countdown in
white text on a compact translucent dark card. Labels move into nearby free space, with thin connector
lines leading back to dots at the exact TA positions. Borders, dots and connector
lines retain faction colours. Cards stay close to their markers when space permits. When card footprints
compete for space and small adjustments cannot resolve it, same-side TAs form a compact stack
showing each TA's name, player and timer. Single cards and stacked rows share the same
translucent background, with blue USA/NATO borders and red USSR borders. Large stacks scroll with the mouse or keyboard;
there is no hover panel to open. Hover or keyboard-focus a single card or stack row to highlight its
exact marker and connector; only the other lines in that stack dim. Stacks prefer the same faction side, mixing
USA/NATO and USSR only as a last resort. At most three rows are visible before
scrolling. Singles stay close to their markers; stacks sit beside the area covered
by all their markers, leaving more room to see the action. Airborne Infantry,
Airdropped Light Tank and Airdropped Transport form separate drop stacks from
other TA where space allows. Single drop cards prefer a 24-pixel gap; other single
TA cards use eight pixels. Very cramped maps may mix categories within a faction
side before resorting to mixed-side stacks. Placement prefers clear space around units, objectives and effects,
and stays stable during countdowns. Stacks shrink as events expire and regroup when
the map is resized. All recognized TA types have countdowns beside the
name, using the shipped marker lifetime and faction-specific values. Timers follow
pause, playback speed and scrubbing. Unknown, unmapped support IDs retain a brief
1.5-second event label without a guessed timer. Marker expiry does not create
units or assert an exact impact or repair time. Unit dots follow recorded creation;
white crosses mark recorded destruction. See the
[complete timer table](../research/findings/tactical-aid-timers-2026-09-11.md).

## Scoreboard and playback

The Replay view includes a live scoreboard grouped by recorded team assignments.
Player scores follow the playback clock, including penalties and backward seeks.
Team totals sum the displayed players' latest recorded scores; an em dash means
one or more scores have not been recorded yet. Spectators and players
whose team is not yet recorded are hidden from the scoreboard. The compact team
cards use Overview's styling and show its orange winner accent once playback
reaches the recorded team-win event. Rewinding before that event removes the accent.

Switching to another detail tab pauses playback and keeps its position. Returning
to Replay leaves it paused at that position; selecting another replay resets it
to the beginning. Playback controls remain available when map art is missing.

## Local data and game files

Browsing, scanning, searching, and loading replay details are read-only operations.
Only an explicit rename or in-game-name edit can modify an original replay; exports
create new copies and retain their sources. Management operations validate their
destinations and rewritten replay structure before committing a change.

The native application stores its SQLite database through
`ProjectDirs("org", "Wicgate", "WiC Replay Viewer")`. On Linux this resolves to:

```text
~/.local/share/wicreplayviewer/library.sqlite3
```

The historical v0.5.0 Flatpak keeps application data separately under
`~/.var/app/org.wicgate.ReplayViewer/`. The v0.6.0 AppImage uses the native path
above, so it does not automatically reuse a library created by the Flatpak.

Technical error reports are saved in separate, bounded storage alongside this
database only after consent in the native dialog. See
[error reporting](error-reporting.md) for the contents and storage policy.

The database contains replay paths, file fingerprints, derived summaries, parser
output, and runtime-decoded map art. It does not contain copies of replay files.
The viewer reads overview art and localization only from folders you select or from
an automatically detected local game installation.

## Error reports

Desktop builds use an independent helper to show native system dialogs for errors,
crashes, and sustained freezes, even if the application window cannot respond.
Choose **Generate local report…** to save and review technical details; dismissing
the dialog saves nothing. Nothing is uploaded.
See [local error reporting](error-reporting.md) for privacy details,
freeze-detection limits, and developer verification.
