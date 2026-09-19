//! Local incident reporting. No API accepts an error message, replay identifier or path
//! as report context. Storage/export paths are control inputs, never report fields.
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const MAX_REPORT_BYTES: usize = 256 * 1024;
const MAX_REPORTS: usize = 10;
const MAX_ENTRIES: usize = 256;
const MAX_AGE: u64 = 30 * 86400;
const MAX_STORAGE: u64 = 5 * 1024 * 1024;
pub mod helper;

static GLOBAL: OnceLock<Arc<Reporter>> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Operation {
    Unknown,
    Startup,
    Import,
    Database,
    Detail,
    Playback,
    MapArt,
    FileOperation,
    Frontend,
    Reporting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Code {
    OperationFailed,
    DatabaseFailed,
    FrontendError,
    UnhandledRejection,
    VueError,
    CallbackFailed,
    NativePanic,
    NativeUiStalled,
    FrontendDeliveryStalled,
    FrameCallbacksStalled,
    WebviewUnresponsive,
    WebviewTerminated,
    UnexpectedExit,
    ReportingStalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Started,
    Finished,
    Cancelled,
    Committed,
    ExpectedProblem,
    Failed,
    Recovered,
}

/// Only app-bundle source positions, never function names or exception text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CodeLocation {
    pub asset: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BuildInfo {
    pub version: String,
    pub fingerprint: String,
    pub revision: String,
    pub os: String,
    pub architecture: String,
    pub assets: Vec<String>,
    pub os_version: Vec<u32>,
    pub webview_version: Vec<u32>,
}

/// Fixed UI categories only; never selected replay identifiers or search text.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum View {
    #[default]
    Unknown,
    Library,
    Overview,
    Replay,
    Chat,
    TacticalAid,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UiContext {
    pub view: View,
    pub focused: bool,
    pub detail_pending: bool,
    pub import_pending: bool,
    pub management_pending: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Observation {
    pub native_age_ms: u64,
    pub frontend_age_ms: u64,
    pub frame_age_ms: u64,
    pub visible: bool,
    pub imported: u64,
    pub rejected: u64,
    pub events_received: u64,
    pub webview_reason: Option<i32>,
    #[serde(default)]
    pub ui: UiContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Entry {
    pub elapsed_ms: u64,
    pub operation: Operation,
    pub phase: Phase,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Report {
    pub schema: u32,
    pub build: BuildInfo,
    pub code: Code,
    pub operation: Operation,
    pub elapsed_ms: u64,
    pub occurrences: u64,
    pub recovered: bool,
    pub observation: Observation,
    pub locations: Vec<CodeLocation>,
    pub history: Vec<Entry>,
    #[serde(default)]
    pub supervisor: helper::SupervisorContext,
}

#[derive(Debug)]
enum Message {
    Activity(Operation, Phase, u64),
    Incident(Code, Operation, Observation, Vec<CodeLocation>),
    Recovered(Code),
    Observation(Observation),
    Stop(mpsc::Sender<()>),
}

pub struct Reporter {
    sender: mpsc::SyncSender<Message>,
    build: BuildInfo,
    session_id: Option<String>,
}

impl Reporter {
    pub fn start_session(build: BuildInfo, session_id: Option<String>) -> Arc<Self> {
        let session_id = session_id.filter(|id| valid_id(id));
        let (sender, receiver) = mpsc::sync_channel(128);
        let reporter = Arc::new(Self {
            sender,
            build: build.clone(),
            session_id: session_id.clone(),
        });
        let _ = std::thread::Builder::new()
            .name("wic-error-reports".into())
            .spawn(move || {
                let Ok(mut session) = Session::new(build, session_id.is_some()) else {
                    return;
                };
                session.snapshot();
                let mut snapshot_at = Instant::now();
                loop {
                    match receiver.recv_timeout(Duration::from_secs(2)) {
                        Ok(Message::Stop(reply)) => {
                            session.clean_exit();
                            let _ = reply.send(());
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            break;
                        }
                        Ok(message) => {
                            session.handle(message);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if snapshot_at.elapsed() >= Duration::from_secs(2) {
                        session.snapshot();
                        snapshot_at = Instant::now();
                    }
                }
            });
        reporter
    }

    pub fn install(self: &Arc<Self>) {
        let _ = GLOBAL.set(self.clone());
    }
    pub fn enabled(&self) -> bool {
        self.session_id.is_some()
    }
    fn send(&self, message: Message) {
        if self.enabled() {
            let _ = self.sender.try_send(message);
        }
    }
    pub fn activity(&self, operation: Operation, phase: Phase, count: u64) {
        self.send(Message::Activity(operation, phase, count));
    }
    pub fn incident(
        &self,
        code: Code,
        operation: Operation,
        observation: Observation,
        locations: Vec<CodeLocation>,
    ) {
        self.send(Message::Incident(code, operation, observation, locations));
    }
    pub fn observe(&self, observation: Observation) {
        self.send(Message::Observation(observation));
    }
    pub fn recovered(&self, code: Code) {
        self.send(Message::Recovered(code));
    }
    pub fn allows_location(&self, asset: &str) -> bool {
        self.build.assets.iter().any(|s| s == asset)
    }
    /// Rust panic hooks cannot rely on another thread draining a queue before exit.
    /// Best effort synchronous, code-only emergency record; no application locks.
    pub fn panic_location(&self, location: Option<CodeLocation>) {
        if !self.enabled() {
            return;
        }
        let report = Report {
            schema: 2,
            build: self.build.clone(),
            code: Code::NativePanic,
            operation: Operation::Unknown,
            elapsed_ms: 0,
            occurrences: 1,
            recovered: false,
            observation: Observation::default(),
            locations: location
                .into_iter()
                .filter(|p| self.allows_location(&p.asset))
                .collect(),
            history: vec![],
            supervisor: helper::SupervisorContext::default(),
        };
        if self.session_id.is_some() {
            helper::emit(&helper::Packet::Incident(report));
        }
    }

    pub fn stop(&self) {
        let (send, receive) = mpsc::channel();
        if self.sender.try_send(Message::Stop(send)).is_ok() {
            let _ = receive.recv_timeout(Duration::from_millis(500));
        }
    }
}

pub fn activity(operation: Operation, phase: Phase, count: u64) {
    if let Some(reporter) = GLOBAL.get() {
        reporter.activity(operation, phase, count);
    }
}
pub fn incident(code: Code, operation: Operation) {
    if let Some(reporter) = GLOBAL.get() {
        reporter.incident(code, operation, Observation::default(), vec![]);
    }
}

#[derive(Clone)]
struct Store {
    directory: PathBuf,
}

fn storage_error(_: impl std::fmt::Debug) -> String {
    "Error report storage is unavailable.".into()
}
fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
/// Session identifiers are internal control values, never exported report fields.
pub fn new_session_id() -> String {
    format!(
        "{}-{}-{}",
        now_seconds(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 80 && id.bytes().all(|b| b.is_ascii_digit() || b == b'-')
}

impl Store {
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("Invalid report identifier.".into());
        }
        Ok(self.directory.join(format!("{id}.json")))
    }
    fn read_report(path: &Path) -> Result<Report, String> {
        if fs::symlink_metadata(path)
            .map_err(storage_error)?
            .file_type()
            .is_symlink()
        {
            return Err(storage_error("symlink"));
        }
        let file = File::open(path).map_err(storage_error)?;
        let mut bytes = Vec::new();
        file.take(MAX_REPORT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(storage_error)?;
        if bytes.len() > MAX_REPORT_BYTES {
            return Err(storage_error("oversized"));
        }
        let report: Report = serde_json::from_slice(&bytes).map_err(storage_error)?;
        validate_report(&report)?;
        Ok(report)
    }
    fn write_report(&self, path: &Path, report: &Report) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(report).map_err(storage_error)?;
        if bytes.len() > MAX_REPORT_BYTES {
            return Err(storage_error("oversized"));
        }
        // Include temporary files in the disk budget.
        let used = fs::read_dir(&self.directory)
            .map_err(storage_error)?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.metadata().ok())
            .filter(|metadata| metadata.is_file())
            .fold(0u64, |total, metadata| total.saturating_add(metadata.len()));
        if used.saturating_add(bytes.len() as u64) > MAX_STORAGE {
            return Err(storage_error("storage budget"));
        }
        let temp = path.with_extension(format!("{}.tmp", SEQUENCE.fetch_add(1, Ordering::Relaxed)));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp).map_err(storage_error)?;
            file.write_all(&bytes).map_err(storage_error)?;
            file.sync_all().map_err(storage_error)?;
            fs::rename(&temp, path).map_err(storage_error)
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
    fn report_files(&self) -> Result<Vec<PathBuf>, String> {
        let mut paths = fs::read_dir(&self.directory)
            .map_err(storage_error)?
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|e| e == "json")
                    && p.file_stem().and_then(|s| s.to_str()).is_some_and(valid_id)
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths.reverse();
        Ok(paths)
    }
    fn prune(&self) -> Result<(), String> {
        let mut size = 0u64;
        for (index, path) in self.report_files()?.iter().enumerate() {
            let metadata = fs::metadata(path).map_err(storage_error)?;
            size = size.saturating_add(metadata.len());
            let age = metadata
                .modified()
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .unwrap_or_default()
                .as_secs();
            if index >= MAX_REPORTS
                || size > MAX_STORAGE
                || metadata.len() > MAX_REPORT_BYTES as u64
                || age > MAX_AGE
            {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
    fn preview(&self, id: &str) -> Result<String, String> {
        serde_json::to_string_pretty(&Self::read_report(&self.path(id)?)?).map_err(storage_error)
    }
    fn export(&self, id: &str, destination: &Path, preview: &str) -> Result<(), String> {
        let current = self.preview(id)?;
        if current != preview {
            return Err("Report changed. Review its updated preview before exporting.".into());
        }
        let directory = fs::canonicalize(&self.directory).map_err(storage_error)?;
        if fs::canonicalize(destination)
            .ok()
            .is_some_and(|p| p.starts_with(&directory))
            || destination
                .parent()
                .and_then(|p| fs::canonicalize(p).ok())
                .is_some_and(|p| p.starts_with(&directory))
        {
            return Err("Choose an export destination outside internal report storage.".into());
        }
        fs::write(destination, current.as_bytes())
            .map_err(|_| "Cannot export the error report.".into())
    }
}

fn validate_build(build: &BuildInfo) -> Result<(), String> {
    if build.os_version.len() > 4 || build.webview_version.len() > 4 {
        return Err(storage_error("version"));
    }
    for s in [
        &build.version,
        &build.fingerprint,
        &build.revision,
        &build.os,
        &build.architecture,
    ] {
        if s.len() > 128
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        {
            return Err(storage_error("build"));
        }
    }
    if build.assets.len() > 1024
        || build.assets.iter().any(|s| {
            s.len() > 160
                || s.contains("..")
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"./_-".contains(&b))
        })
    {
        return Err(storage_error("assets"));
    }
    Ok(())
}

fn validate_report(report: &Report) -> Result<(), String> {
    if !matches!(report.schema, 1 | 2)
        || report.history.len() > MAX_ENTRIES
        || report.locations.len() > 32
        || report.supervisor.timeline.len() > MAX_ENTRIES
    {
        return Err(storage_error("schema"));
    }
    validate_build(&report.build)?;
    if report
        .locations
        .iter()
        .any(|p| !report.build.assets.contains(&p.asset))
    {
        return Err(storage_error("location"));
    }
    Ok(())
}

struct Session {
    build: BuildInfo,
    started: Instant,
    transport: bool,
    history: VecDeque<Entry>,
    reports: Vec<Report>,
    observation: Observation,
    #[cfg(test)]
    store: Store,
}

impl Session {
    fn new(build: BuildInfo, transport: bool) -> Result<Self, String> {
        validate_build(&build)?;
        Ok(Self {
            build,
            started: Instant::now(),
            transport,
            history: VecDeque::new(),
            reports: vec![],
            observation: Observation::default(),
            #[cfg(test)]
            store: Store {
                directory: PathBuf::new(),
            },
        })
    }
    #[cfg(test)]
    fn open(store: Store, build: BuildInfo) -> Result<Self, String> {
        let mut session = Self::new(build, false)?;
        session.store = store;
        Ok(session)
    }
    fn send(&self, packet: &helper::Packet) {
        if self.transport {
            helper::emit(packet);
        }
    }
    fn elapsed(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    #[cfg(test)]
    fn reset(&mut self) {
        self.history.clear();
        self.reports.clear();
        self.observation = Observation::default();
        self.started = Instant::now();
    }
    fn base(&self, code: Code, operation: Operation) -> Report {
        Report {
            schema: 2,
            build: self.build.clone(),
            code,
            operation,
            elapsed_ms: self.elapsed(),
            occurrences: 1,
            recovered: false,
            observation: self.observation.clone(),
            locations: vec![],
            history: self.history.iter().cloned().collect(),
            supervisor: helper::SupervisorContext::default(),
        }
    }
    fn handle(&mut self, message: Message) {
        match message {
            Message::Activity(operation, phase, count) => {
                if operation == Operation::Import {
                    if phase == Phase::Started {
                        self.observation.imported = 0;
                        self.observation.rejected = 0;
                    }
                    if phase == Phase::Committed {
                        self.observation.imported = self.observation.imported.saturating_add(count);
                    }
                    if phase == Phase::ExpectedProblem {
                        self.observation.rejected = self.observation.rejected.saturating_add(count);
                    }
                }
                if let Some(last) = self.history.back_mut()
                    && last.operation == operation
                    && last.phase == phase
                {
                    last.count = last.count.saturating_add(count);
                } else {
                    self.history.push_back(Entry {
                        elapsed_ms: self.elapsed(),
                        operation,
                        phase,
                        count,
                    });
                }
                while self.history.len() > MAX_ENTRIES {
                    self.history.pop_front();
                }
            }
            Message::Observation(mut observation) => {
                observation.imported = self.observation.imported;
                observation.rejected = self.observation.rejected;
                self.observation = observation;
            }
            Message::Incident(code, operation, mut observation, locations) => {
                observation.imported = self.observation.imported;
                observation.rejected = self.observation.rejected;
                self.observation = observation;
                if let Some(report) = self
                    .reports
                    .iter_mut()
                    .find(|r| r.code == code && r.operation == operation && !r.recovered)
                {
                    report.occurrences = report.occurrences.saturating_add(1);
                    report.observation = self.observation.clone();
                    let packet = helper::Packet::Incident(report.clone());
                    self.send(&packet);
                    return;
                }
                let mut report = self.base(code, operation);
                report.locations = locations
                    .into_iter()
                    .filter(|p| self.build.assets.contains(&p.asset))
                    .take(32)
                    .collect();
                self.send(&helper::Packet::Incident(report.clone()));
                self.reports.push(report);
                if self.reports.len() > MAX_REPORTS {
                    self.reports.remove(0);
                }
            }
            Message::Recovered(code) => {
                for report in self.reports.iter_mut().filter(|r| r.code == code) {
                    report.recovered = true;
                }
                self.send(&helper::Packet::Recovered(code));
            }
            Message::Stop(_) => {}
        }
    }
    fn snapshot(&self) {
        self.send(&helper::Packet::Snapshot(
            self.base(Code::UnexpectedExit, Operation::Unknown),
        ));
    }
    fn clean_exit(&self) {
        self.send(&helper::Packet::CleanExit);
    }
}

/// Pure watchdog state machine. Clock gaps reset baselines, never assert a cause.
#[derive(Default)]
pub struct Watchdog {
    previous_tick: u64,
    baseline: u64,
    hidden_at: u64,
    active: Vec<Code>,
}
impl Watchdog {
    pub fn tick(
        &mut self,
        now: u64,
        observation: &Observation,
        enabled: bool,
    ) -> Vec<(Code, bool)> {
        if !enabled || now.saturating_sub(self.previous_tick) > 5000 {
            self.baseline = now;
            self.active.clear();
        }
        self.previous_tick = now;
        if !observation.visible {
            self.hidden_at = now;
        }
        let frontend_visible = observation.visible && now.saturating_sub(self.hidden_at) >= 15_000;
        let ready = enabled && now >= 30_000 && now.saturating_sub(self.baseline) >= 15_000;
        let conditions = [
            (
                Code::NativeUiStalled,
                true,
                observation.native_age_ms >= 15_000,
            ),
            (
                Code::FrontendDeliveryStalled,
                frontend_visible,
                observation.frontend_age_ms >= 15_000,
            ),
            (
                Code::FrameCallbacksStalled,
                frontend_visible && observation.frontend_age_ms < 15_000,
                observation.frame_age_ms >= 15_000,
            ),
        ];
        let mut changes = vec![];
        for (code, monitored, stale) in conditions {
            let present = self.active.contains(&code);
            if ready && monitored && stale && !present {
                self.active.push(code);
                changes.push((code, false));
            }
            if present && ready && monitored && !stale {
                self.active.retain(|c| *c != code);
                changes.push((code, true));
            }
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    pub(super) fn build() -> BuildInfo {
        BuildInfo {
            version: "0.4.0".into(),
            fingerprint: "abc123".into(),
            revision: "unknown".into(),
            os: "linux".into(),
            architecture: "x86_64".into(),
            assets: vec!["assets/index-abc.js".into(), "src/lib.rs".into()],
            os_version: vec![6, 19],
            webview_version: vec![2, 52],
        }
    }
    fn session(path: &Path) -> Session {
        Session::open(
            Store {
                directory: path.into(),
            },
            build(),
        )
        .unwrap()
    }
    fn fault(s: &mut Session, code: Code) {
        s.handle(Message::Incident(
            code,
            Operation::Frontend,
            Observation::default(),
            vec![],
        ));
    }

    #[test]
    fn incidents_are_bounded_coalesced_and_export_contains_only_code_locations() {
        let dir = tempdir().unwrap();
        let mut s = session(dir.path());
        for i in 0..300 {
            s.handle(Message::Activity(
                Operation::Import,
                if i % 2 == 0 {
                    Phase::Started
                } else {
                    Phase::Finished
                },
                1,
            ));
        }
        s.handle(Message::Activity(Operation::Import, Phase::Committed, 64));
        s.handle(Message::Activity(
            Operation::Import,
            Phase::ExpectedProblem,
            3,
        ));
        s.handle(Message::Activity(
            Operation::Import,
            Phase::ExpectedProblem,
            2,
        ));
        s.handle(Message::Incident(
            Code::FrontendError,
            Operation::Frontend,
            Observation::default(),
            vec![
                CodeLocation {
                    asset: "/home/Alice/private-replay.wicdemo".into(),
                    line: 3,
                    column: 1,
                },
                CodeLocation {
                    asset: "assets/index-abc.js".into(),
                    line: 42,
                    column: 7,
                },
            ],
        ));
        fault(&mut s, Code::FrontendError);
        assert_eq!(s.reports.len(), 1);
        assert_eq!(s.reports[0].occurrences, 2);
        let preview = serde_json::to_string(&s.reports[0]).unwrap();
        assert!(!preview.contains("Alice"));
        assert!(!preview.contains("wicdemo"));
        let report = &s.reports[0];
        assert_eq!(report.locations.len(), 1);
        assert_eq!(report.history.len(), MAX_ENTRIES);
        assert_eq!(report.observation.imported, 64);
        assert_eq!(report.observation.rejected, 5);
        s.handle(Message::Recovered(Code::FrontendError));
        assert!(s.reports[0].recovered);
        fault(&mut s, Code::FrontendError);
        assert_eq!(s.reports.len(), 2);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn expected_problems_never_create_incident_files() {
        let dir = tempdir().unwrap();
        let mut s = session(dir.path());
        for _ in 0..10_000 {
            s.handle(Message::Activity(
                Operation::Import,
                Phase::ExpectedProblem,
                1,
            ));
        }
        assert!(
            s.store
                .report_files()
                .unwrap()
                .iter()
                .all(|p| Store::read_report(p).is_err())
        );
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.observation.rejected, 10_000);
        s.handle(Message::Observation(Observation {
            frontend_age_ms: 2000,
            visible: true,
            ..Observation::default()
        }));
        assert_eq!(s.observation.rejected, 10_000);
        assert_eq!(s.observation.frontend_age_ms, 2000);
        s.snapshot();
        s.reset();
        assert!(s.history.is_empty());
    }

    #[test]
    fn malformed_oversized_and_private_fields_are_rejected() {
        let dir = tempdir().unwrap();
        let s = session(dir.path());
        let path = s.store.path("1").unwrap();
        let base = s.base(Code::OperationFailed, Operation::Startup);
        let mut value = serde_json::to_value(&base).unwrap();
        value["message"] = serde_json::json!("Alice private chat /home/alice");
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(s.store.preview("1").is_err());
        fs::write(&path, b"incomplete{").unwrap();
        assert!(s.store.preview("1").is_err());
        fs::write(&path, vec![b'x'; MAX_REPORT_BYTES + 1]).unwrap();
        assert!(s.store.preview("1").is_err());
        let mut report = base.clone();
        report.locations.push(CodeLocation {
            asset: "/private/secret".into(),
            line: 0,
            column: 0,
        });
        s.store.write_report(&path, &report).unwrap();
        assert!(s.store.preview("1").is_err());
        report = base.clone();
        report.schema = 99;
        s.store.write_report(&path, &report).unwrap();
        assert!(s.store.preview("1").is_err());
        report = base.clone();
        report.build.fingerprint = "../../private".into();
        s.store.write_report(&path, &report).unwrap();
        assert!(s.store.preview("1").is_err());
        report = base;
        report.history = vec![
            Entry {
                elapsed_ms: 0,
                operation: Operation::Startup,
                phase: Phase::Started,
                count: 0
            };
            MAX_ENTRIES + 1
        ];
        s.store.write_report(&path, &report).unwrap();
        assert!(s.store.preview("1").is_err());
        assert!(
            s.store
                .report_files()
                .unwrap()
                .iter()
                .all(|p| Store::read_report(p).is_err())
        );
    }

    #[test]
    fn retention_and_io_failures_are_bounded() {
        let dir = tempdir().unwrap();
        let s = session(dir.path());
        for i in 100..120 {
            s.store
                .write_report(
                    &s.store.path(&i.to_string()).unwrap(),
                    &s.base(Code::NativePanic, Operation::Startup),
                )
                .unwrap();
        }
        s.store.prune().unwrap();
        assert_eq!(s.store.report_files().unwrap().len(), MAX_REPORTS);
        let old = s.store.path("999").unwrap();
        s.store
            .write_report(&old, &s.base(Code::NativePanic, Operation::Startup))
            .unwrap();
        File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH))
            .unwrap();
        s.store.prune().unwrap();
        assert!(!old.exists());
        let bad = Store {
            directory: dir.path().join("not-a-directory"),
        };
        fs::write(&bad.directory, "x").unwrap();
        assert!(bad.prune().is_err());
        assert!(
            s.store
                .write_report(
                    &dir.path().join("absent/report.json"),
                    &s.base(Code::NativePanic, Operation::Startup)
                )
                .is_err()
        );
        let mut huge = s.base(Code::NativePanic, Operation::Startup);
        huge.build.assets = vec!["x".repeat(MAX_REPORT_BYTES)];
        assert!(
            s.store
                .write_report(&s.store.path("900").unwrap(), &huge)
                .is_err()
        );
    }

    #[test]
    fn recorder_and_panic_do_not_write_without_consent() {
        let dir = tempdir().unwrap();
        let destination = dir.path().join("reports");
        let r = Reporter::start_session(build(), None);
        r.panic_location(Some(CodeLocation {
            asset: "src/lib.rs".into(),
            line: 5,
            column: 1,
        }));
        r.incident(
            Code::VueError,
            Operation::Frontend,
            Observation::default(),
            vec![],
        );
        r.activity(Operation::Startup, Phase::Started, 1);
        r.recovered(Code::NativeUiStalled);
        r.stop();
        assert!(!destination.exists());
    }

    #[test]
    fn watchdog_reports_observations_once_and_resets_on_suspend_and_disable() {
        let mut w = Watchdog::default();
        let mut o = Observation {
            visible: true,
            ..Observation::default()
        };
        for now in (0..30_000).step_by(2000) {
            assert!(w.tick(now, &o, true).is_empty());
        }
        o.native_age_ms = 16_000;
        o.frontend_age_ms = 16_000;
        assert_eq!(
            w.tick(30_000, &o, true),
            vec![
                (Code::NativeUiStalled, false),
                (Code::FrontendDeliveryStalled, false)
            ]
        );
        assert!(w.tick(32_000, &o, true).is_empty());
        o.native_age_ms = 0;
        o.frontend_age_ms = 0;
        o.frame_age_ms = 16_000;
        let changes = w.tick(34_000, &o, true);
        assert!(changes.contains(&(Code::FrameCallbacksStalled, false)));
        o.visible = false;
        assert!(w.tick(36_000, &o, true).is_empty());
        o.visible = true;
        assert!(w.tick(100_000, &o, true).is_empty());
        for now in (102_000..116_000).step_by(2000) {
            assert!(w.tick(now, &o, true).is_empty());
        }
        assert_eq!(
            w.tick(116_000, &o, true),
            vec![(Code::FrameCallbacksStalled, false)]
        );
        assert!(w.tick(118_000, &o, false).is_empty());
        assert!(w.tick(120_000, &o, true).is_empty());
    }

    #[test]
    fn restoring_a_hidden_window_allows_fresh_frame_acknowledgements() {
        let mut watchdog = Watchdog::default();
        let mut observation = Observation {
            visible: false,
            frame_age_ms: 120_000,
            ..Observation::default()
        };
        for now in (0..60_000).step_by(2000) {
            assert!(watchdog.tick(now, &observation, true).is_empty());
        }
        observation.visible = true;
        assert!(watchdog.tick(60_000, &observation, true).is_empty());
        observation.frame_age_ms = 0;
        for now in (62_000..80_000).step_by(2000) {
            assert!(watchdog.tick(now, &observation, true).is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn report_symlinks_are_not_read_or_listed() {
        let dir = tempdir().unwrap();
        let s = session(dir.path());
        let target = dir.path().join("secret");
        fs::write(&target, "private").unwrap();
        std::os::unix::fs::symlink(target, s.store.path("123").unwrap()).unwrap();
        assert!(s.store.preview("123").is_err());
        assert!(
            s.store
                .report_files()
                .unwrap()
                .iter()
                .all(|p| Store::read_report(p).is_err())
        );
    }
}
