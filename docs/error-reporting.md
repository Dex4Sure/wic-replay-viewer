# Local error reports

Desktop builds show a native system dialog when an unexpected application error,
crash, or sustained freeze is detected. There is no error-report footer or in-app
report manager. Choose **Generate local report…** to save a report on this device
and review it. **Keep waiting**, **Dismiss**, or closing the dialog saves nothing.
Generation consent applies only to that incident; it does not enable automatic
reporting for future errors. Export uses a separate native save dialog. Nothing is
uploaded; share an exported file only when you choose.

Before consent, bounded technical context stays in process memory and travels to
the independent helper through an anonymous pipe. No report, diagnostic log, or
session checkpoint is written to disk. Previously generated reports remain in
`error-reports` beside `library.sqlite3`; removing that folder does not change the
replay library. Existing reports from older builds are not opened automatically.
The former recording preference is no longer used: saving always requires the
incident dialog's explicit consent. Unavailable storage must not prevent normal
application use.

When keeping exported reports in this checkout, use `src-tauri/errors/`. Git and
Prettier ignore that entire directory and its contents. This is a developer
storage convention; generated reports still use the application data directory,
and the export dialog still lets you choose a destination.

## What a report contains

The schema accepts fixed error codes, operation categories, relative elapsed
milliseconds, bounded operation counts, and known application source positions.
Build metadata includes the product version, source fingerprint, Git revision,
operating-system family and architecture. Numeric kernel and WebKit versions are
currently available on Linux; an empty version array means unknown. The fingerprint
identifies application source, never a replay or user.

Reports exclude error messages, arbitrary exception text, function names, user or
machine names, filesystem paths, replay names, player names, chat, replay bytes,
replay hashes, settings, environment variables, console output, screenshots, and
memory dumps. Expected failures such as corrupt replays and missing directories
contribute aggregate counts instead of generating an incident for every file.
Database failures, Rust panics, unhandled frontend errors/rejections, Vue errors,
and failed application callbacks have dedicated reporting paths. New features
must use these typed reporting APIs; do not add raw logging to the export schema.

Each incident contains at most 256 operation-history entries, 256 supervisor
lifecycle entries, 32 source positions, and
256 KiB of JSON. Repeated incidents of the same code and operation are coalesced
in memory. Panic hooks send best-effort code-only context through the pipe without
waiting for the recorder queue; these omit operation history. After consent,
storage retains at most ten reports, pruning older than 30 days when a report is
generated, with a 5 MiB write budget including temporary files. Reports use atomic
replacement. Context collection and transport queues are bounded; extreme bursts
may drop context. Parsing never waits for diagnostic queue space.

## Freeze and exit evidence (schema 2)

Reports now include fixed UI context: library/detail tab, document focus, and
whether detail loading, import, or file management is pending. These are the last
received heartbeat values, not proof that an operation caused a freeze.

The supervisor keeps a bounded timeline of incidents, recoveries, clean-shutdown
markers, close requests and process exit. Its elapsed times use the supervisor's
clock; the report's existing history uses the application's clock. Exit evidence
includes the numeric exit code, Unix signal/core-dump flag when available, clean
shutdown marker and whether the helper requested closure. A signal alone does not
establish who sent it or prove an out-of-memory kill. The core-dump flag does not
mean a dump file was collected or is available. Windows records the numeric
process exit code; its Unix signal is null and Unix core-dump flag is false.
The shared lifecycle and UI context are available on Windows as well as Linux.

A warning left open includes subsequent lifecycle evidence when **Generate local
report…** is selected. Exported reports are snapshots and are never silently
rewritten after consent. Unexpected exit has a separate notification that waits
for queue space instead of being discarded behind an earlier warning. Old schema
1 reports remain readable. This does not automatically diagnose graphics-driver,
compositor or WebKit internals, and does not restart the viewer.

### Optional native stack capture on Linux

For a live freeze, explicitly run:

```bash
bash scripts/capture-viewer-stacks.sh APPLICATION_CHILD_PID
```

Use the viewer application's PID with `--wic-application-session` in its command
line, not the supervisor PID. The script verifies the executable name and child
marker, invokes GDB without init scripts or automatic symbol downloads, briefly
pauses the process, captures up to 32 frames per native thread, then detaches.
It writes to a private directory under `local/generated/viewer-stacks/` with the
executable SHA-256. It never runs automatically or attaches to the game. Host
ptrace permissions and installed GDB are required; failure details stay in the
capture directory. It does not change host ptrace policy.

This is a separate opt-in diagnostic artifact: it may contain local code paths
and symbol names. It excludes argument/local-variable printing and memory dumps;
review it before sharing. It captures application-native threads, not JavaScript
stacks or separate WebKit processes. Matching native debug symbols improve its
usefulness. After a process has exited, live stack capture is impossible; a
separately available OS crash dump and matching symbols are needed for a crash
backtrace. The reporter does not enable or collect OS memory dumps.

## Independent crash and freeze helper

The desktop executable first starts a small supervisor process, which launches the
application as its child. The supervisor never starts Tauri or a webview. It stays
alive to observe the child's exit and to display reports when the app cannot do so.
Both modes ship in the same executable, so Windows portable ZIPs and Linux
AppImages do not need a separately located sidecar. The AppImage remains mounted
while the supervisor is alive; its child inherits the bundled library environment.
The AppImage launcher selects dark GTK dialogs to match the viewer unless an
explicit `APPIMAGE_GTK_THEME` or `GTK_THEME` override is provided.

A serious frontend error, native panic, webview failure, sustained UI stall, or
unexpected process exit opens a native notification. It explains the observation
and offers **Generate local report…**, **Keep waiting** or **Dismiss**, and **Close app** when
the child is still running. Closing requires a second confirmation and stops any
unfinished parsing. Closing the app does not grant report-generation consent.
Unexpected database and callback errors can also offer a report; expected import
rejections contribute counts without opening dialogs. Notifications from a single
ongoing stall are grouped.

The report preview is a separate, read-only native text view. Export opens a native
save dialog and writes only the exact preview after verifying it has not changed.
Linux uses GTK controls and the available desktop theme; Windows uses native task
dialogs and Win32 controls; macOS uses AppKit. No game styling, HTML, or webview is
used by these windows. Exact appearance depends on the platform and installed theme.

The supervisor also notices when the recorder's pipe updates stop arriving. It
offers a **Reporting stalled** report, preserving the last actual measurements. This means
that diagnostic updates stopped; it does not prove that the UI itself froze. A
scheduling gap resets the supervisor's timeout baseline to avoid suspend false
positives. Multiple instances have separate child pipes and in-memory state.

## Detection and interpretation

A separate Rust thread checks native UI acknowledgements independently of the
JavaScript bridge. The frontend sends a heartbeat with a sampled animation-frame
acknowledgement every two seconds. At most one native probe, bridge request, and
frame callback is outstanding. A 15-second stall after a 30-second startup grace
period produces an observation; later acknowledgements end the active stall
episode in memory. An already generated report remains the snapshot approved by
the user and is not updated automatically after recovery. Hidden or
minimized frontend windows are excluded, with a grace period after restoring them.
A watchdog scheduling gap resets the
baseline to avoid classifying suspend or whole-process pauses as a UI freeze.
Linux WebKit and Windows WebView2 also provide native web-process failure signals.
macOS uses the portable heartbeat monitoring.

An **Unexpected exit** notice means the helper observed the application exit
without a successful clean shutdown. The exit code or signal can narrow the cause,
but does not by itself distinguish every crash, force quit or external termination. A heartbeat observation identifies what stopped responding,
not its root cause. GPU rendering can fail even when frame callbacks continue.
These reports are not crash dumps. Detection never restarts the application or
interrupts an import automatically. If the helper or entire sandbox is also killed,
or power is lost, unsaved context is lost. There is deliberately no next-launch
recovery checkpoint before consent. A full disk can prevent a requested report
from being saved.

## Developer verification and symbols

Portable tests cover retention, privacy allowlists, export/preview equality,
coalescing, bounded pipe parsing, no disk writes before consent, watchdog timing,
legacy schema compatibility, and lifecycle updates while a warning stays open.
A native adapter regression checks terminal delivery through a full notification
queue. The debug example exercises real desktop stalls without adding a release fault switch:

```bash
npm run build
cargo build -p wic-replay-viewer-app --example error_reporting_probe --features custom-protocol
XDG_DATA_HOME=$(mktemp -d) target/debug/examples/error_reporting_probe javascript
```

Other scenarios are `healthy`, `native`, `panic`, `exit`, and `import <folder>`.
Use an isolated `XDG_DATA_HOME`; never point these probes at a real library profile.
The probe waits 35 seconds, injects a 20-second stall where applicable, then allows
30 seconds for recovery before exiting. `exit` deliberately terminates without
cleanup, while its supervisor remains alive to offer report generation.
The injection entry point and automatic dialog-capture/response test controls are
compiled out when debug assertions are disabled. For reproducible Linux desktop
probes of the independent helper, run:

```bash
python3 scripts/error-reporting-helper-probe.py javascript
python3 scripts/error-reporting-helper-probe.py native
python3 scripts/error-reporting-helper-probe.py exit
python3 scripts/error-reporting-helper-probe.py exit --decline
python3 scripts/error-reporting-helper-probe.py whole-process
```

These opt-in probes create an empty profile and native window captures under ignored
`artifacts/helper-probes/`. They never import replays. The debug UI responds with generation consent by default; `--decline` dismisses
the dialog and verifies that no diagnostic storage was created. The last probe suspends only
its verified application child while checking that the supervisor still saves a
report and renders a notification, then resumes the child. Use a desktop session;
these are not part of the display-independent portable gate.
Keep the debug import scenario to small samples. Test large libraries in an
optimized desktop build; debug parsing speed is not a production benchmark.

The September 10 consent-flow validation passed the JavaScript freeze, whole-app
suspension, and crash-decline probes on Fedora. Declining created no diagnostic
directory; accepting generated a report and native preview. A separate Flatpak
crash probe verified that the helper survived the child exit and offered generation.
Those results describe the September 10 build. The large debug import was
cancelled and is not evidence about the original rare freeze.

On September 12, the schema 2 changes passed the full portable quality gate and
the terminal-notification regression. The Linux stack-capture command captured
both threads of a synthetic native process and detached successfully; this was
not a reproduction of the viewer freeze.

The complete Windows MSVC release executable also cross-built successfully with
`scripts/build-windows-cross.sh`. Runtime helper tests used Bottles Soda 11.0-10
in the isolated `WiC-Report-Validation` bottle. A synthetic harness compiled
byte-identical copies of the updated helper and Win32 preview, linked the actual
diagnostics core and application Common Controls v6 resource, and interacted only
with its own windows:

- Crash plus **Dismiss** created no new or changed reports.
- A synthetic frame-stall incident, recovery, and exit code 42 appeared in both
  the original warning's report and the subsequent unexpected-exit report.
- Tactical Aid tab/focus context survived, and native read-only previews opened.

Local evidence, screenshots, source hashes and the harness are under ignored
`artifacts/windows-reporter-validation-2026-09-12/`. These Wine tests validate the
helper, not the full Tauri/WebView2 application, Windows-host graphics/DPI, or the
native export/save dialog. Those still require Windows-host testing. AppKit was
previously type-checked; macOS runtime appearance checks remain outstanding.
None of these tests establishes the cause of the reported viewer freeze.

Builds place provenance and frontend source maps in `artifacts/diagnostic-symbols`.
The frontend collector removes maps from `dist` before Tauri embeds it. AppImage
builds retain native symbols under `diagnostic-symbols/linux/`, alongside frontend
maps and build provenance, separately from the public bundle;
Windows builds retain a PDB separately from the portable executable ZIP. Desktop
CI uploads these as `diagnostic-symbols-*` artifacts with 90-day retention. They
are excluded from public release artifact selection. Archive matching artifacts
for builds that need support beyond that retention period.

Use the report fingerprint and revision to select the matching source and symbols.
Frontend positions reference an exact bundled asset and a one-based line/column;
resolve them using its matching source map (source-map columns are zero-based).
Rust positions are recorded only when they unambiguously match known repository
source. Third-party, ambiguous, and unknown locations are omitted. Native symbols
are useful for a separately authorized debugger investigation; these privacy-limited
reports do not contain raw native addresses or thread stacks.

The portable `src/diagnostics/helper.rs` monitor remains in the viewer-core coverage
floor. Native process and widget adapters in `src-tauri/src/error_helper*` are
excluded from portable line coverage alongside the existing Tauri runtime adapter;
they are checked through platform compilation and the opt-in desktop probes above.
