# Bulk import responsiveness

The September 2026 installed-Flatpak report described a frozen frontend while Rust
continued committing summaries. Static review identified three weaknesses addressed
here; it did not establish the native hang's root cause.

## Event delivery and recovery

The importer now emits `summariesStored` with an array of at most 64 committed rows,
replacing the individual `summaryStored` payload. Failed transactions still retry
rows individually and notify only successfully stored rows. The final partial batch
is delivered before `importFinished`. Parser output, cache keys, and database schemas
are unchanged; the viewer and embedded backend must be built together.

The frontend updates progress once per batch and keeps incoming rows outside Vue
reactivity. A two-second `import_running` heartbeat runs until an idle backend has
been reconciled with SQLite. It does not read the library during parsing. Recovery
also runs after a normal completion, so lost row events are repaired even if the
completion event arrives. `initial_state` reads run on a native blocking worker.
Requests are serialized within a recovery generation, failures retry, and old
responses cannot replace a newer scan or update an unmounted window. Snapshots also retry if rows or locations change while loading,
so late recovery cannot undo a replay-management or folder update.

Recovery prunes missing rows and selections and clears stale import controls.
Without a completion event, it conservatively retains folders marked as needing a
scan because a boolean idle state cannot prove whether the scan was cancelled.
Callback and backend event-delivery failures also contribute typed diagnostic
context in memory. A stalled or crashed webview cannot execute this JavaScript
recovery heartbeat. The separate [error-reporting watchdog](error-reporting.md)
observes native UI, bridge, and frame acknowledgements on a Rust thread. An
independent helper process receives bounded context through a private pipe and
can display a native dialog even when the application cannot respond. A report is
saved only after the user chooses **Generate local report…**; dismissing the dialog
saves nothing. Schema 2 reports also retain a bounded supervisor lifecycle timeline,
process exit details and the last received UI context. These can distinguish the
observed stall from a later exit, without proving that import or rendering caused
it. Detection does not restart the application or interrupt parsing.

## Completion preparation

Bulk merge and stable sorting use 256-row chunks and bounded merge steps separated
by timer tasks, allowing the browser to process input and paint. Search metadata,
library statistics, and folder groups are prepared in chunks before publishing one
shallow-reactive summary array. Weak caches retain immutable identities without
keeping obsolete libraries alive. Empty search avoids evaluating the search index.
Startup and recovery snapshots use the same projection preparation.

The old library remains visible and scan controls remain active until preparation
finishes. Generation checks reject obsolete completion work. Virtualization still
limits rendered rows; parser workers, lazy detail loading, and source replay files
are unchanged.

## Portable regression coverage

- A native synthetic importer test stores 130 rejected replay inputs, verifies that
  notifications follow SQLite commits, and checks event sizes of 64, 64, and 2.
- Recovery tests cover missing notifications, slow requests, transient errors,
  backend-state races, restarts of monitoring, and unmount cleanup.
- A mounted Vue test delivers 4,096 summaries in batches and waits for asynchronous
  completion before checking the published rows and import controls.
- A 10,000-summary test compares merge/removal, statistics, folder grouping, and
  search with the synchronous reference functions while a timer heartbeat runs.
  Sort tests exercise chunk boundaries, stable ties, and unchanged inputs.

Run `./scripts/quality.sh full` for the portable suite and coverage floors. These
synthetic checks do not measure the installed Flatpak webview or real parser memory
pressure. The private corpus was unavailable during the macOS validation recorded
below. Injected JavaScript and native UI stalls have since been detected and marked
recovered on Fedora, including inside the Flatpak runtime. These controlled probes
do not reproduce or establish the cause of the original intermittent import freeze.
Use an optimized desktop build for a large real import. If a failure occurs, choose
**Generate local report…** in the native dialog and inspect the report; controlled
probes alone do not establish that the original issue is resolved. Keep debug import probes
small; they are not representative of production parsing performance.

## macOS validation record

The September 10 portable checks passed 223 frontend tests and their coverage floors,
130 Python tests, and 230 Rust tests with five private installed-game tests ignored.
Rust line coverage was 91.94% for the parser, 84.96% for the viewer core, and 100% for
the Tauri command core. Production frontend build and strict Clippy passed.

Validation exposed existing macOS gate issues: GNU-only replay counting, temporary
path aliases, and use of the system Python 3.9. The counting and path assumptions
were corrected; tests used Python 3.14.6 and pinned Ruff 0.16.3. The existing parallel
corpus CLI test intermittently reported missing fixtures on its second invocation.
Its temporary directories used timestamp-only names within one process, allowing
concurrent tests to share and remove a directory if timestamps collided. Names now
include an atomic sequence and creation is exclusive; assertions include stderr for
any recurrence. Parser behavior was not changed by this test-harness correction.
