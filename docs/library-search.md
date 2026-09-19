# Library search

Search reads the summaries already loaded into memory. Typing never opens replay
files, queries SQLite, or loads replay details.

## Syntax

| Query                          | Matches                                             |
| ------------------------------ | --------------------------------------------------- |
| `generalx riviera`             | Every word somewhere in searchable metadata         |
| `player:generalx map:riviera`  | A GeneralX result entry on Riviera                  |
| `player:generalx faction:nato` | A GeneralX result entry whose faction is NATO       |
| `generalx nato riviera`        | GeneralX playing NATO on Riviera, with any recorder |
| `riviera ussr`                 | Riviera replays containing USSR on either team      |
| `player:"Weed Queen"`          | A result player whose name contains Weed Queen      |
| `"exact phrase"`               | Consecutive text within one metadata field          |

Matching ignores case. Plain terms, player names, and maps use substring matching.
Bare faction words and faction filters use the complete faction name: USA, NATO, or USSR.
When a query includes a faction, plain terms matching player names in the loaded
library become player constraints. Each such term must match a result player with
that faction; a filename or recorder name alone cannot establish the association.
Other terms continue matching across metadata. Use explicit fields to disambiguate
a map or other metadata word that also occurs in a player name.
With no explicit or recognized player name, `faction:nato` means NATO appears in
the replay. With player constraints, the faction must match each named player's own result entry. Multiple
player filters require each name to be present; they can match different entries.
Unknown factions never satisfy a faction filter. Filter order does not matter.

Bare words retain the existing searchable fields: filename, in-game replay name,
server, internal/display map name, game/server mode, format, date, recorder,
player names, represented factions, and parse error. Winner and loser status are
display information only; there is no winner filter. Full directory paths are not
searched. Unsupported prefixes and minus signs are literal text, not operators. An unfinished quoted value can
match while typing; an empty filter such as `player:` does not match everything.

Server and format terms still combine with player/faction/map terms. For example,
`generalx nato riviera ranked 4vs4` requires GeneralX playing NATO, plus matching
Riviera, ranked, and 4vs4 metadata. Format matching uses the stored text; `4vs4`
and `4v4` are not aliases. There are no `server:` or `format:` operators.
Formats count all selected result players on playing teams, including zero-score
players, in every server mode. Spectators remain excluded; unknown teams leave the
format unknown. The existing duplicate and overflow-departure cleanup still applies.
Cache `detail-v48` marks older summaries and details stale. Use **Refresh library**
to recalculate existing library formats.

The indexed players are the selected results shown by Overview, including its
historical fallback policy. This is not a search of every person who appeared
at any time during playback.

## Caching and refresh

Database schema 13 adds a JSON `search_players` column. Imports preserve each
selected result player's name and optional faction together. The frontend receives
these as `searchPlayers`. Migration retains existing summaries and cached details,
but marks summaries stale so **Refresh library** can populate the new pairs.
Until a row is refreshed, ordinary text search without a player/faction combination
still works; player/faction combinations and `player:` filters cannot match its
missing structured data. No identities are inferred by splitting
old comma-separated names or combining them with a replay's faction list.

The frontend normalizes each summary once, reuses unchanged prepared entries
through a weak-reference cache, and rebuilds an entry when import or rename replaces
its summary object. Removed entries can be garbage-collected. A separate computed
query parses once per input change. Filtering only inspects prepared strings;
the existing virtualized replay list renders the results. Startup, bulk completion,
and automatic recovery prepare metadata in yielding chunks before publishing rows.
Summaries use shallow reactivity to retain their immutable identity. An empty query
bypasses the computed search index rather than forcing a whole-library traversal.
See [bulk import responsiveness](bulk-import-responsiveness.md) for validation and
remaining native-runtime checks.

Product version and parser JSON schema are unchanged.

## Historical performance check

Before automatic player/faction matching was introduced, timings were measured
locally on Node 24.18.0 using a read-only snapshot of the installed library's
**4,089 summaries**. Each query ran 20 warmups followed by 100 timed runs.
Timings include query parsing and filtering, but exclude browser rendering and
initial database loading. At that time, original and prepared plain searches
returned identical paths.
These timings and counts do not measure the current automatic association behavior.

| Query                   | Previous median | Prepared median | Prepared p95 |
| ----------------------- | --------------: | --------------: | -----------: |
| `generalx`              |         2.33 ms |         0.28 ms |      0.33 ms |
| `generalx riviera`      |         2.56 ms |         0.30 ms |      0.31 ms |
| `generalx riviera nato` |         2.65 ms |         0.31 ms |      0.35 ms |
| `ussr`                  |         2.43 ms |         0.42 ms |      0.57 ms |
| `no-such-player`        |         2.30 ms |         0.50 ms |      0.52 ms |

Preparing the real metadata took 11.70 ms once. The old library snapshot did not
contain the new structured players. A separate, explicitly synthetic stress case
added 16 player/faction pairs to every real summary (65,424 pairs). Tested typed
queries had medians of 0.22–0.76 ms and p95 values of 0.23–0.85 ms. These are local
measurements, not guarantees for every machine or query.

Run the benchmark against a local JSON array of serialized summaries with:

```bash
node scripts/benchmark-search.node.mjs /path/to/summaries.json
```

Browser controls at that time also verified that `player:generalx map:riviera faction:nato` and
its USSR equivalent choose different rows, while `generalx riviera` returns both.
The private benchmark input and raw measurements remain ignored under
`local/generated/search-benchmark/`.

## Historical real player/faction control

A focused native import check regenerated search projections for the 46 library
entries matching `generalx riviera`. Original files were read without modification;
source paths and SHA-256 values were recorded in the ignored benchmark directory.
The queries returned:

| Query                                      | Matches |
| ------------------------------------------ | ------: |
| `generalx riviera`                         |      46 |
| `player:generalx map:riviera`              |      31 |
| `player:generalx map:riviera faction:nato` |      23 |
| `player:generalx map:riviera faction:ussr` |       6 |

The other two player matches list GeneralX as a spectator. Plain metadata matches
can come from filenames or replay titles; `player:` requires a selected result
occupant. Counts refer to library entries, including multiple copies of a replay.
These projections were generated only for validation; normal searching never
performs this import work.

The full portable quality gate, private regressions (including summary/detail
player-pair parity), Quarry playback control, and all 17 ground-truth fixtures passed.

## Automatic association verification

The updated search was checked against the existing 46-entry GeneralX/Riviera
summary snapshot. `generalx nato riviera` returns the same 23 replay paths as
`player:generalx faction:nato map:riviera`; the USSR equivalents return the same
6 paths. This reuses stored projections, without reimporting or modifying replays.
All 234 frontend tests, type checking, and lint passed. The local browser preview
verified the updated search tips; search results were checked through the search
function against stored native summaries, without running the desktop application.
