//! Dedicated developer executable. No injection entry point exists in release builds.
#[cfg(debug_assertions)]
fn main() {
    let mut args = std::env::args().skip(1);
    let scenario = args
        .next()
        .expect("scenario: healthy, javascript, native, panic, exit, import");
    wic_replay_viewer_app::run_error_reporting_probe(scenario, args.next());
}
#[cfg(not(debug_assertions))]
fn main() {
    panic!("Error reporting probes require a debug build");
}
