//! Supervisor runs before Tauri, owns the application Child, and never starts a webview.
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
use wic_replay_viewer::diagnostics::{
    Code,
    helper::{Monitor, Notice, ProcessExit, read_packets},
    new_session_id,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

const CHILD_ARGUMENT: &str = "--wic-application-session";
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Wait,
    Generate,
    CloseApp,
}
enum Event {
    Notice(Box<Notice>),
    Ended,
}

fn send_notice(events: &mpsc::SyncSender<Event>, notice: Notice, terminal: bool) {
    let event = Event::Notice(Box::new(notice));
    if terminal {
        // Terminal evidence must survive an open dialog and a full queue.
        // The child has exited, so waiting cannot stop live monitoring.
        let _ = events.send(event);
    } else {
        let _ = events.try_send(event);
    }
}

pub(super) fn child_session() -> Option<String> {
    let args = std::env::args().collect::<Vec<_>>();
    let index = args.iter().rposition(|a| a == CHILD_ARGUMENT)?;
    let id = args.get(index + 1)?;
    (!id.is_empty() && id.len() < 80 && id.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
        .then(|| id.clone())
}

pub(super) fn supervise() -> bool {
    if child_session().is_some() {
        return false;
    }
    let Some(dirs) = directories::ProjectDirs::from("org", "Wicgate", "WiC Replay Viewer") else {
        return false;
    };
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let directory = dirs.data_dir().join("error-reports");
    let id = new_session_id();
    let build = super::reporting::build_info();
    let Ok(mut monitor) = Monitor::new(build) else {
        return false;
    };
    let Ok(mut child) = Command::new(executable)
        .args(std::env::args_os().skip(1))
        .arg(CHILD_ARGUMENT)
        .arg(&id)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
    else {
        return false;
    };
    let output = child.stdout.take().expect("piped child stdout");
    let (packets, input) = mpsc::sync_channel(32);
    let reader = std::thread::spawn(move || {
        read_packets(output, |packet| {
            let _ = packets.send(packet);
        });
    });
    let alive = Arc::new(AtomicBool::new(true));
    let monitor_alive = alive.clone();
    let (events, receive) = mpsc::sync_channel(1);
    let (close, controls) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let started = Instant::now();
        let mut closed_by_user = false;
        loop {
            if controls.try_recv().is_ok() {
                closed_by_user = true;
                monitor.close_requested(started.elapsed().as_millis() as u64);
                // The handle belongs to this exact child. Never find/kill a process by PID.
                let _ = child.kill();
            }
            let status = match child.try_wait() {
                Ok(status) => status,
                Err(_) => {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };
            // Once the child exits, allow the pipe to drain its final clean-exit marker.
            if status.is_some() {
                while let Ok(packet) = input.recv_timeout(Duration::from_millis(100)) {
                    monitor.ingest(packet, started.elapsed().as_millis() as u64);
                }
            } else {
                for packet in input.try_iter().take(64) {
                    monitor.ingest(packet, started.elapsed().as_millis() as u64);
                }
            }
            if let Some(status) = status {
                #[cfg(unix)]
                let (signal, core_dumped) = {
                    use std::os::unix::process::ExitStatusExt;
                    (status.signal(), status.core_dumped())
                };
                #[cfg(not(unix))]
                let (signal, core_dumped) = (None, false);
                monitor.process_exit(ProcessExit {
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    code: status.code(),
                    signal,
                    core_dumped,
                    clean_shutdown: false,
                    close_requested: closed_by_user,
                });
            }
            monitor_alive.store(status.is_none(), Ordering::Release);
            if let Some(notice) = monitor.tick(
                started.elapsed().as_millis() as u64,
                status.is_none(),
                status.is_some_and(|s| s.success()),
            ) && !closed_by_user
            {
                send_notice(&events, notice, status.is_some());
            }
            if status.is_some() {
                let _ = events.send(Event::Ended);
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    while let Ok(event) = receive.recv() {
        let Event::Notice(notice) = event else {
            break;
        };
        match notification(&notice, alive.load(Ordering::Acquire)) {
            Action::Generate => match notice.generate(&directory) {
                Ok((id, preview)) => {
                    while preview_report(&preview) {
                        let Some(destination) = choose_export() else {
                            continue;
                        };
                        if Notice::export(&directory, &id, &destination, &preview).is_ok() {
                            break;
                        }
                        information(
                            "Report not exported",
                            "The destination may be unavailable. Choose another location to try again.",
                        );
                    }
                }
                Err(_) => information(
                    "Report not generated",
                    "The local report could not be saved. Check available disk space and folder permissions.",
                ),
            },
            Action::CloseApp if alive.load(Ordering::Acquire) && confirm_close() => {
                let _ = close.try_send(());
            }
            _ => {}
        }
    }
    let _ = worker.join();
    // Do not wait on unrelated descendants that inherited stdout.
    drop(reader);
    true
}

fn description(notice: &Notice, alive: bool) -> (&'static str, String) {
    let (title, explanation) = if !alive {
        (
            "WiC Replay Viewer closed unexpectedly",
            "A problem was detected when the application closed. Previously saved replays are kept.",
        )
    } else if notice.code == Code::ReportingStalled {
        (
            "WiC Replay Viewer may be unresponsive",
            "The application stopped sending diagnostic updates. Background work may still be running.",
        )
    } else if matches!(
        notice.code,
        Code::NativeUiStalled
            | Code::FrontendDeliveryStalled
            | Code::FrameCallbacksStalled
            | Code::WebviewUnresponsive
    ) {
        (
            "WiC Replay Viewer stopped responding",
            "A responsiveness problem was detected. Background parsing may still be running, and the application may recover if you wait.",
        )
    } else {
        (
            "WiC Replay Viewer encountered a problem",
            "An unexpected application error was detected. The application may still be usable.",
        )
    };
    (
        title,
        format!(
            "{explanation}\n\nGenerate a technical report on this device? It contains app/build details, error codes and recent activity counts. It excludes replay contents, names and file paths. Nothing is uploaded. No report is saved unless you choose Generate local report."
        ),
    )
}

#[cfg(target_os = "linux")]
use linux::{choose_export, confirm_close, information, notification, preview_report};

#[cfg(not(target_os = "linux"))]
fn information(title: &str, text: &str) {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(text)
        .show();
}
#[cfg(not(target_os = "linux"))]
fn notification(notice: &Notice, alive: bool) -> Action {
    let (title, text) = description(notice, alive);
    let buttons = if alive {
        rfd::MessageButtons::YesNoCancelCustom(
            "Generate local report…".into(),
            "Close app…".into(),
            "Keep waiting".into(),
        )
    } else {
        rfd::MessageButtons::OkCancelCustom("Generate local report…".into(), "Dismiss".into())
    };
    match rfd::MessageDialog::new()
        .set_title(title)
        .set_description(text)
        .set_level(rfd::MessageLevel::Warning)
        .set_buttons(buttons)
        .show()
    {
        rfd::MessageDialogResult::Custom(s) if s == "Generate local report…" => Action::Generate,
        rfd::MessageDialogResult::Custom(s) if s == "Close app…" => Action::CloseApp,
        _ => Action::Wait,
    }
}
#[cfg(not(target_os = "linux"))]
fn confirm_close() -> bool {
    rfd::MessageDialog::new()
        .set_title("Close WiC Replay Viewer?")
        .set_description(
            "Closing the app stops any parsing still running. Previously saved replays are kept.",
        )
        .set_buttons(rfd::MessageButtons::OkCancelCustom(
            "Close app".into(),
            "Keep waiting".into(),
        ))
        .show()
        == rfd::MessageDialogResult::Custom("Close app".into())
}
#[cfg(not(target_os = "linux"))]
fn choose_export() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export error report")
        .set_file_name("WiCReplayViewer-error-report.json")
        .add_filter("Error report", &["json"])
        .save_file()
}
#[cfg(target_os = "macos")]
use macos::preview_report;
#[cfg(windows)]
use windows::preview_report;
#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn preview_report(_: &str) -> bool {
    false
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    #[test]
    fn terminal_notice_survives_a_full_notification_queue() {
        let mut monitor = Monitor::new(super::super::reporting::build_info()).unwrap();
        let notice = monitor.tick(1000, false, false).unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        send.send(Event::Ended).unwrap();
        let worker = std::thread::spawn(move || send_notice(&send, notice, true));
        assert!(matches!(receive.recv().unwrap(), Event::Ended));
        let Event::Notice(notice) = receive.recv_timeout(Duration::from_secs(5)).unwrap() else {
            panic!("terminal evidence lost");
        };
        assert_eq!(notice.code, Code::UnexpectedExit);
        worker.join().unwrap();
    }
}
