import assert from 'node:assert/strict';
import test from 'node:test';

import { enforceRustCoverage, summarizeRustCoverage } from './rust-coverage.mjs';

function file(filename, count, covered) {
  return {
    filename: new URL(`../${filename}`, import.meta.url).pathname,
    summary: { lines: { count, covered } },
  };
}

function report(files) {
  return {
    type: 'llvm.coverage.json.export',
    data: [{ files }],
  };
}

test('summarizes the three existing crate floors while excluding binary adapters', () => {
  const summaries = summarizeRustCoverage(
    report([
      file('parser/rust_parser/src/lib.rs', 100, 85),
      file('parser/rust_parser/src/main.rs', 100, 0),
      file('src/lib.rs', 100, 80),
      file('src-tauri/src/command_core.rs', 100, 75),
      file('src-tauri/src/lib.rs', 100, 0),
      file('src-tauri/src/main.rs', 100, 0),
      file('src-tauri/src/error_helper.rs', 100, 0),
      file('src-tauri/src/error_helper/linux.rs', 100, 0),
      file('src-tauri/src/error_helper/windows.rs', 100, 0),
      file('src-tauri/src/error_helper/macos.rs', 100, 0),
    ]),
  );

  assert.deepEqual(
    summaries.map(({ label, count, covered, percent }) => ({ label, count, covered, percent })),
    [
      { label: 'parser', count: 100, covered: 85, percent: 85 },
      { label: 'viewer core', count: 100, covered: 80, percent: 80 },
      { label: 'Tauri command core', count: 100, covered: 75, percent: 75 },
    ],
  );
});

test('fails closed when a required crate has no measured source', () => {
  assert.throws(
    () => summarizeRustCoverage(report([file('src/lib.rs', 100, 100)])),
    /no measured lines for parser/,
  );
});

test('rejects a crate below its existing line floor', () => {
  assert.throws(
    () =>
      enforceRustCoverage(
        report([
          file('parser/rust_parser/src/lib.rs', 100, 84),
          file('src/lib.rs', 100, 80),
          file('src-tauri/src/command_core.rs', 100, 75),
        ]),
      ),
    /Rust line coverage floor not met/,
  );
});
