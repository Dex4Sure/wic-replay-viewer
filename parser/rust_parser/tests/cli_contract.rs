use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::{Compression, write::ZlibEncoder};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "wic-replay-parser-cli-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create CLI test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_minimal_valid_replay(path: &Path) {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    // One complete 21-byte Event envelope whose message is TeamWins. Match-result
    // extraction tolerates absent optional fields, so this is sufficient to
    // exercise batch behavior while satisfying the parser's structural gate.
    encoder
        .write_all(&[
            0x03, 0x02, 0xb5, 0x05, // Event
            0x15, 0x00, 0x00, 0x00, // envelope type
            0x06, // array flag
            21, 0x00, 0x00, 0x00, // total envelope bytes
            0x00, 0x00, 0x00, 0x00, // time = 0.0f
            0x29, 0x03, 0xb8, 0x0d, // TeamWins
        ])
        .expect("compress TeamWins marker");
    let compressed = encoder.finish().expect("finish replay chunk");

    let mut replay = vec![0u8; 19];
    replay.extend_from_slice(&compressed);
    fs::write(path, replay).expect("write valid replay fixture");
}

#[test]
fn json_batch_exits_nonzero_when_any_input_fails() {
    let directory = TestDir::new();
    let valid = directory.path().join("valid.wicdemo");
    let invalid = directory.path().join("invalid.wicdemo");
    write_minimal_valid_replay(&valid);
    fs::write(&invalid, [0u8; 20]).expect("write invalid replay fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_wic_replay_parser"))
        .arg("--json")
        .arg(&valid)
        .arg(&invalid)
        .output()
        .expect("run native parser");

    assert!(!output.status.success());
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("partial batch remains valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Error parsing invalid.wicdemo"));
}

#[test]
fn native_cli_rejects_an_existing_non_wicdemo_path() {
    let directory = TestDir::new();
    let invalid = directory.path().join("replay.dat");
    fs::write(&invalid, [0u8; 20]).expect("write wrong-extension fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_wic_replay_parser"))
        .arg("--json")
        .arg(&invalid)
        .output()
        .expect("run native parser");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must end in .wicdemo"));
}

#[test]
fn directory_glob_and_explicit_inputs_are_deduplicated_in_order() {
    let directory = TestDir::new();
    let first = directory.path().join("a.wicdemo");
    let second = directory.path().join("b.wicdemo");
    write_minimal_valid_replay(&first);
    write_minimal_valid_replay(&second);
    fs::write(directory.path().join("ignored.txt"), b"ignored").expect("text fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_wic_replay_parser"))
        .arg("--json")
        .arg("--dir")
        .arg(directory.path())
        .arg(&first)
        .arg(directory.path().join("*.wicdemo"))
        .output()
        .expect("run native parser");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("batch JSON");
    assert_eq!(
        rows.len(),
        2,
        "overlapping collectors must not duplicate files"
    );
}

#[test]
fn recursive_corpus_output_is_identical_across_worker_counts_and_roots() {
    let directory = TestDir::new();
    let nested = directory.path().join("nested");
    fs::create_dir(&nested).expect("nested directory");
    write_minimal_valid_replay(&directory.path().join("b.wicdemo"));
    write_minimal_valid_replay(&nested.join("a.wicdemo"));

    let run = |jobs: &str, duplicate_root: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_wic_replay_parser"));
        command.arg("--corpus-summary-json").arg(directory.path());
        if duplicate_root {
            command.arg(directory.path());
        }
        command.arg("--jobs").arg(jobs);
        command.output().expect("run corpus parser")
    };

    let serial = run("1", false);
    let parallel = run("4", true);
    assert!(
        serial.status.success(),
        "serial: {}",
        String::from_utf8_lossy(&serial.stderr)
    );
    assert!(
        parallel.status.success(),
        "parallel: {}",
        String::from_utf8_lossy(&parallel.stderr)
    );
    assert_eq!(serial.stdout, parallel.stdout);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&serial.stdout).expect("corpus JSON");
    assert_eq!(rows.len(), 2);
    let paths: Vec<&str> = rows.iter().filter_map(|row| row["path"].as_str()).collect();
    assert!(paths[0] < paths[1], "corpus output must be path-sorted");
}
