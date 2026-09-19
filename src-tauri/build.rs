use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn collect(root: &Path, directory: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(directory).expect("source directory").flatten() {
        let path = entry.path();
        if entry.file_type().expect("source type").is_dir() {
            collect(root, &path, files);
        } else if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| matches!(s, "rs" | "ts" | "vue" | "css" | "json" | "toml" | "lock"))
        {
            files.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn main() {
    let root = Path::new("..");
    let mut files = vec![
        "Cargo.toml".into(),
        "Cargo.lock".into(),
        "package.json".into(),
        "package-lock.json".into(),
        "vite.config.ts".into(),
        "src-tauri/Cargo.toml".into(),
        "src-tauri/build.rs".into(),
        "src-tauri/tauri.conf.json".into(),
    ];
    for directory in ["src", "src-tauri/src", "frontend", "parser/rust_parser/src"] {
        collect(root, &root.join(directory), &mut files);
    }
    files.sort();
    let mut digest = Sha256::new();
    for file in &files {
        println!("cargo:rerun-if-changed=../{file}");
        digest.update(file.as_bytes());
        digest.update([0]);
        digest.update(fs::read(root.join(file)).expect("build source"));
        digest.update([0]);
    }
    let fingerprint = format!("{:x}", digest.finalize());
    let revision = std::env::var("WIC_BUILD_REVISION")
        .ok()
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        })
        .filter(|v| v.len() == 40 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .or_else(|| {
            fs::read_to_string(root.join(".build-revision"))
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| v.len() == 40 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        })
        .unwrap_or_else(|| "unknown".into());
    let mut assets = files
        .into_iter()
        .filter(|s| s.ends_with(".rs"))
        .collect::<Vec<_>>();
    if let Ok(entries) = fs::read_dir(root.join("dist/assets")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".js") {
                assets.push(format!("assets/{name}"));
            }
        }
    }
    println!("cargo:rerun-if-changed=../dist/assets");
    println!("cargo:rerun-if-env-changed=WIC_BUILD_REVISION");
    println!("cargo:rustc-env=WIC_BUILD_FINGERPRINT={fingerprint}");
    println!("cargo:rustc-env=WIC_BUILD_REVISION={revision}");
    println!(
        "cargo:rustc-env=WIC_REPORT_ASSETS={}",
        serde_json::to_string(&assets).unwrap()
    );
    let symbols = root.join("artifacts/diagnostic-symbols");
    fs::create_dir_all(&symbols).expect("symbol metadata directory");
    fs::write(symbols.join("build.json"), serde_json::to_vec_pretty(&serde_json::json!({ "fingerprint": fingerprint, "revision": revision, "version": env!("CARGO_PKG_VERSION"), "assets": assets })).unwrap()).expect("symbol build metadata");
    tauri_build::build();
}
