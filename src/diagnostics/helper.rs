//! Private, bounded pipe transport and in-memory monitoring. Disk writes require consent.
use super::*;
use std::io::BufRead;
use std::sync::Mutex;

const PREFIX: &[u8] = b"WIC_DIAGNOSTIC_V1 ";
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Packet {
    Snapshot(Report),
    Incident(Report),
    Recovered(Code),
    CleanExit,
}

pub(super) fn emit(packet: &Packet) {
    let Ok(bytes) = serde_json::to_vec(packet) else {
        return;
    };
    if bytes.len() > MAX_REPORT_BYTES {
        return;
    }
    // stdout is an anonymous pipe owned by the supervisor, not a log file.
    let mut output = std::io::stdout().lock();
    let _ = output
        .write_all(PREFIX)
        .and_then(|_| output.write_all(&bytes))
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush());
}

/// Discard unrelated output without retaining or logging it. Bound each frame even
/// when a misbehaving dependency prints a line without a newline.
pub fn read_packets(input: impl Read, mut receive: impl FnMut(Packet)) {
    let mut input = std::io::BufReader::new(input);
    let mut line = Vec::new();
    let mut oversized = false;
    while let Ok(buffer) = input.fill_buf() {
        if buffer.is_empty() {
            break;
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let length = end.map_or(buffer.len(), |i| i + 1);
        if !oversized && line.len() + length <= MAX_REPORT_BYTES + PREFIX.len() + 1 {
            line.extend_from_slice(&buffer[..length]);
        } else {
            oversized = true;
            line.clear();
        }
        input.consume(length);
        if end.is_some() {
            if !oversized
                && let Some(bytes) = line.strip_prefix(PREFIX)
                && let Ok(packet) = serde_json::from_slice(bytes)
            {
                receive(packet);
            }
            line.clear();
            oversized = false;
        }
    }
}

/// Times in this section use the supervisor clock, independent of child timestamps.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SupervisorContext {
    pub timeline: Vec<LifecycleEntry>,
    pub exit: Option<ProcessExit>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProcessExit {
    pub elapsed_ms: u64,
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub core_dumped: bool,
    pub clean_shutdown: bool,
    pub close_requested: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LifecycleEntry {
    pub elapsed_ms: u64,
    pub code: Option<Code>,
    pub phase: LifecyclePhase,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifecyclePhase {
    Incident,
    Recovered,
    CleanShutdown,
    CloseRequested,
    Exited,
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub code: Code,
    report: Report,
    context: Arc<Mutex<SupervisorContext>>,
}
impl Notice {
    /// The caller invokes this only after the user selects Generate local report.
    pub fn generate(&self, directory: &Path) -> Result<(String, String), String> {
        let mut report = self.report.clone();
        report.schema = 2;
        report.supervisor = self
            .context
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        validate_report(&report)?;
        fs::create_dir_all(directory).map_err(storage_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
                .map_err(storage_error)?;
        }
        let store = Store {
            directory: directory.into(),
        };
        store.prune()?;
        let id = new_session_id();
        store.write_report(&store.path(&id)?, &report)?;
        store.prune()?;
        Ok((id.clone(), store.preview(&id)?))
    }
    pub fn export(
        directory: &Path,
        id: &str,
        destination: &Path,
        preview: &str,
    ) -> Result<(), String> {
        Store {
            directory: directory.into(),
        }
        .export(id, destination, preview)
    }
}

pub struct Monitor {
    build: BuildInfo,
    previous_tick: u64,
    fresh_at: u64,
    last: Option<Report>,
    finished: bool,
    clean: bool,
    active: Vec<Code>,
    pending: VecDeque<Notice>,
    context: Arc<Mutex<SupervisorContext>>,
}
fn stall(code: Code) -> bool {
    matches!(
        code,
        Code::NativeUiStalled
            | Code::FrontendDeliveryStalled
            | Code::FrameCallbacksStalled
            | Code::WebviewUnresponsive
            | Code::ReportingStalled
    )
}
impl Monitor {
    pub fn new(build: BuildInfo) -> Result<Self, String> {
        validate_build(&build)?;
        Ok(Self {
            build,
            previous_tick: 0,
            fresh_at: 0,
            last: None,
            finished: false,
            clean: false,
            active: vec![],
            pending: VecDeque::new(),
            context: Arc::new(Mutex::new(SupervisorContext::default())),
        })
    }
    pub fn ingest(&mut self, packet: Packet, now: u64) {
        match packet {
            Packet::Snapshot(report) | Packet::Incident(report)
                if validate_report(&report).is_err() => {}
            Packet::Snapshot(report) => {
                self.fresh_at = now;
                if self.active.contains(&Code::ReportingStalled) {
                    self.record(now, Some(Code::ReportingStalled), LifecyclePhase::Recovered);
                }
                self.active.retain(|c| *c != Code::ReportingStalled);
                self.last = Some(report);
            }
            Packet::Incident(report) => {
                self.record(now, Some(report.code), LifecyclePhase::Incident);
                self.last = Some(report.clone());
                if !report.recovered && !self.active.contains(&report.code) {
                    let coalesced = stall(report.code) && self.active.iter().any(|c| stall(*c));
                    self.active.push(report.code);
                    if !coalesced && self.pending.len() < MAX_REPORTS {
                        self.pending.push_back(Notice {
                            code: report.code,
                            report,
                            context: self.context.clone(),
                        });
                    }
                }
            }
            Packet::Recovered(code) => {
                self.record(now, Some(code), LifecyclePhase::Recovered);
                self.active.retain(|c| *c != code);
                self.pending.retain(|n| n.code != code);
            }
            Packet::CleanExit => {
                self.clean = true;
                self.record(now, None, LifecyclePhase::CleanShutdown);
            }
        }
    }
    fn notice(&self, code: Code, now: u64) -> Notice {
        let mut report = self.last.clone().unwrap_or_else(|| Report {
            schema: 2,
            build: self.build.clone(),
            code,
            operation: Operation::Unknown,
            elapsed_ms: now,
            occurrences: 1,
            recovered: false,
            observation: Observation::default(),
            locations: vec![],
            history: vec![],
            supervisor: SupervisorContext::default(),
        });
        report.code = code;
        report.operation = Operation::Unknown;
        report.recovered = false;
        Notice {
            code,
            report,
            context: self.context.clone(),
        }
    }
    fn record(&self, now: u64, code: Option<Code>, phase: LifecyclePhase) {
        let mut context = self.context.lock().unwrap_or_else(|e| e.into_inner());
        if context.timeline.len() == MAX_ENTRIES {
            context.timeline.remove(0);
        }
        context.timeline.push(LifecycleEntry {
            elapsed_ms: now,
            code,
            phase,
        });
    }
    pub fn close_requested(&self, now: u64) {
        self.record(now, None, LifecyclePhase::CloseRequested);
    }
    pub fn process_exit(&self, mut exit: ProcessExit) {
        exit.clean_shutdown = self.clean;
        self.record(exit.elapsed_ms, None, LifecyclePhase::Exited);
        self.context.lock().unwrap_or_else(|e| e.into_inner()).exit = Some(exit);
    }
    /// Liveness comes from the owned Child handle. No report/checkpoint is written here.
    pub fn tick(&mut self, now: u64, alive: bool, successful_exit: bool) -> Option<Notice> {
        if self.finished {
            return None;
        }
        if now.saturating_sub(self.previous_tick) > 5000 {
            self.fresh_at = now;
        }
        self.previous_tick = now;
        if !alive {
            self.finished = true;
            return if successful_exit && self.clean {
                None
            } else {
                Some(self.notice(Code::UnexpectedExit, now))
            };
        }
        if now >= 30_000
            && now.saturating_sub(self.fresh_at) >= 15_000
            && !self.active.contains(&Code::ReportingStalled)
        {
            let coalesced = self.active.iter().any(|c| stall(*c));
            self.active.push(Code::ReportingStalled);
            self.record(now, Some(Code::ReportingStalled), LifecyclePhase::Incident);
            if !coalesced {
                return Some(self.notice(Code::ReportingStalled, now));
            }
        }
        self.pending.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn report(code: Code) -> Report {
        Session::new(super::super::tests::build(), false)
            .unwrap()
            .base(code, Operation::Frontend)
    }
    fn monitor() -> Monitor {
        Monitor::new(super::super::tests::build()).unwrap()
    }
    #[test]
    fn consent_is_the_only_disk_write_and_export_is_exact() {
        let dir = tempdir().unwrap();
        let storage = dir.path().join("reports");
        let mut m = monitor();
        m.ingest(Packet::Incident(report(Code::FrontendError)), 2);
        let notice = m.tick(2, true, false).unwrap();
        assert!(!storage.exists()); // showing/dismissing has no persistence
        assert!(m.tick(4, true, false).is_none());
        let (id, preview) = notice.generate(&storage).unwrap();
        assert_eq!(fs::read_dir(&storage).unwrap().count(), 1);
        let output = dir.path().join("export.json");
        Notice::export(&storage, &id, &output, &preview).unwrap();
        assert_eq!(fs::read_to_string(output).unwrap(), preview);
        assert!(Notice::export(&storage, &id, &dir.path().join("other"), "changed").is_err());
        assert!(Notice::export(&storage, &id, &storage.join("bad.json"), &preview).is_err());
        assert!(Notice::export(&storage, "../private", &dir.path().join("bad"), &preview).is_err());
    }
    #[test]
    fn open_notice_captures_later_recovery_and_exact_exit_at_consent() {
        let dir = tempdir().unwrap();
        let mut m = monitor();
        m.ingest(
            Packet::Incident(report(Code::FrameCallbacksStalled)),
            64_000,
        );
        let notice = m.tick(64_000, true, false).unwrap();
        m.ingest(Packet::Recovered(Code::FrameCallbacksStalled), 66_000);
        m.close_requested(67_000);
        m.process_exit(ProcessExit {
            elapsed_ms: 68_000,
            code: None,
            signal: Some(9),
            core_dumped: false,
            clean_shutdown: false,
            close_requested: true,
        });
        let (_, preview) = notice.generate(dir.path()).unwrap();
        let saved: Report = serde_json::from_str(&preview).unwrap();
        assert_eq!(saved.schema, 2);
        assert_eq!(saved.code, Code::FrameCallbacksStalled);
        assert_eq!(saved.supervisor.timeline.len(), 4);
        assert!(matches!(
            saved.supervisor.timeline[1].phase,
            LifecyclePhase::Recovered
        ));
        let exit = saved.supervisor.exit.unwrap();
        assert_eq!(exit.signal, Some(9));
        assert_eq!(exit.code, None);
        assert!(exit.close_requested);
        assert!(!exit.clean_shutdown);
        assert_eq!(exit.elapsed_ms, 68_000);
    }
    #[test]
    fn lifecycle_is_bounded_and_clean_exit_is_preserved() {
        let mut m = monitor();
        for now in 0..1000 {
            m.ingest(Packet::Recovered(Code::FrameCallbacksStalled), now);
        }
        m.ingest(Packet::CleanExit, 1000);
        m.process_exit(ProcessExit {
            elapsed_ms: 1001,
            code: Some(0),
            signal: None,
            core_dumped: false,
            clean_shutdown: false,
            close_requested: false,
        });
        let context = m.context.lock().unwrap();
        assert_eq!(context.timeline.len(), MAX_ENTRIES);
        assert_eq!(context.timeline.last().unwrap().elapsed_ms, 1001);
        assert!(context.exit.as_ref().unwrap().clean_shutdown);
    }
    #[test]
    fn legacy_reports_remain_readable_and_ui_rejects_arbitrary_strings() {
        let mut value = serde_json::to_value(report(Code::FrameCallbacksStalled)).unwrap();
        value["schema"] = 1.into();
        value.as_object_mut().unwrap().remove("supervisor");
        value["observation"].as_object_mut().unwrap().remove("ui");
        let old: Report = serde_json::from_value(value.clone()).unwrap();
        validate_report(&old).unwrap();
        value["observation"]["ui"] = serde_json::json!({
            "view": "/home/alice/private.wicdemo", "focused": true,
            "detailPending": false, "importPending": false, "managementPending": false
        });
        assert!(serde_json::from_value::<Report>(value).is_err());
    }
    #[test]
    fn pipe_is_bounded_and_rejects_unstructured_private_output() {
        let mut input = b"raw private error /home/alice\n".to_vec();
        input.extend(vec![b'x'; MAX_REPORT_BYTES * 2]);
        input.push(b'\n');
        input.extend(PREFIX);
        input.extend(serde_json::to_vec(&Packet::Snapshot(report(Code::UnexpectedExit))).unwrap());
        input.push(b'\n');
        input.extend(PREFIX);
        input.extend(b"{bad}\n");
        let mut packets = vec![];
        read_packets(input.as_slice(), |p| packets.push(p));
        assert_eq!(packets.len(), 1);
    }
    #[test]
    fn freezes_coalesce_and_recovery_allows_a_new_episode() {
        let mut m = monitor();
        m.ingest(Packet::Incident(report(Code::NativeUiStalled)), 0);
        m.ingest(Packet::Incident(report(Code::FrameCallbacksStalled)), 0);
        assert_eq!(m.tick(0, true, false).unwrap().code, Code::NativeUiStalled);
        assert!(m.tick(2000, true, false).is_none());
        m.ingest(Packet::Recovered(Code::NativeUiStalled), 2000);
        m.ingest(Packet::Recovered(Code::FrameCallbacksStalled), 2000);
        m.ingest(Packet::Incident(report(Code::NativeUiStalled)), 4000);
        assert!(m.tick(4000, true, false).is_some());
    }
    #[test]
    fn missing_stream_preserves_actual_observations_and_suspend_resets_baseline() {
        let mut m = monitor();
        m.ingest(Packet::Snapshot(report(Code::UnexpectedExit)), 0);
        for now in (0..30_000).step_by(2000) {
            assert!(m.tick(now, true, false).is_none());
        }
        let notice = m.tick(30_000, true, false).unwrap();
        assert_eq!(notice.code, Code::ReportingStalled);
        assert_eq!(notice.report.observation.native_age_ms, 0);
        assert!(m.tick(32_000, true, false).is_none());
        m.ingest(Packet::Snapshot(report(Code::UnexpectedExit)), 34_000);
        assert!(matches!(
            m.context.lock().unwrap().timeline.last().unwrap().phase,
            LifecyclePhase::Recovered
        ));
        assert!(m.tick(34_000, true, false).is_none());
        assert!(m.tick(120_000, true, false).is_none());
    }
    #[test]
    fn exit_without_clean_shutdown_is_offered_even_when_no_snapshot_arrived() {
        let mut m = monitor();
        assert_eq!(m.tick(0, false, false).unwrap().code, Code::UnexpectedExit);
        assert!(m.tick(1, false, false).is_none());
        let mut m = monitor();
        m.ingest(Packet::CleanExit, 0);
        assert!(m.tick(0, false, true).is_none());
    }
    #[test]
    fn invalid_packets_and_recovered_errors_do_not_notify() {
        let mut m = monitor();
        let mut invalid = report(Code::NativePanic);
        invalid.locations.push(CodeLocation {
            asset: "/private/file".into(),
            line: 1,
            column: 1,
        });
        m.ingest(Packet::Incident(invalid), 0);
        assert!(m.tick(0, true, false).is_none());
        m.ingest(Packet::Incident(report(Code::DatabaseFailed)), 0);
        assert_eq!(m.tick(0, true, false).unwrap().code, Code::DatabaseFailed);
        m.ingest(Packet::Recovered(Code::DatabaseFailed), 0);
        let mut recovered = report(Code::DatabaseFailed);
        recovered.recovered = true;
        m.ingest(Packet::Incident(recovered), 0);
        assert!(m.tick(0, true, false).is_none());
    }
}
