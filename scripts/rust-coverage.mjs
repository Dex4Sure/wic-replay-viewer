import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = fileURLToPath(new URL('..', import.meta.url));

export const coverageGroups = [
  {
    label: 'parser',
    prefix: 'parser/rust_parser/src/',
    excluded: new Set(['parser/rust_parser/src/main.rs']),
    minimum: 85,
  },
  {
    label: 'viewer core',
    prefix: 'src/',
    excluded: new Set(),
    minimum: 80,
  },
  {
    label: 'Tauri command core',
    prefix: 'src-tauri/src/',
    excluded: new Set([
      'src-tauri/src/lib.rs',
      'src-tauri/src/main.rs',
      'src-tauri/src/error_helper.rs',
      'src-tauri/src/error_helper/linux.rs',
      'src-tauri/src/error_helper/windows.rs',
      'src-tauri/src/error_helper/macos.rs',
    ]),
    minimum: 75,
  },
];

function relativeFilename(filename) {
  return path.relative(repoRoot, path.resolve(filename)).split(path.sep).join('/');
}

export function summarizeRustCoverage(report) {
  if (report?.type !== 'llvm.coverage.json.export' || !Array.isArray(report.data)) {
    throw new Error('Expected an LLVM coverage JSON export.');
  }

  const files = report.data.flatMap((entry) => entry.files ?? []);
  return coverageGroups.map((group) => {
    const matchingFiles = files.filter((file) => {
      const filename = relativeFilename(file.filename);
      return filename.startsWith(group.prefix) && !group.excluded.has(filename);
    });
    const count = matchingFiles.reduce((total, file) => total + file.summary.lines.count, 0);
    const covered = matchingFiles.reduce((total, file) => total + file.summary.lines.covered, 0);
    if (count === 0) {
      throw new Error(`Coverage report contains no measured lines for ${group.label}.`);
    }
    return {
      ...group,
      count,
      covered,
      percent: (covered / count) * 100,
    };
  });
}

export function enforceRustCoverage(report) {
  const summaries = summarizeRustCoverage(report);
  const failures = [];
  for (const summary of summaries) {
    const result =
      `${summary.label}: ${summary.percent.toFixed(2)}% ` +
      `(${summary.covered}/${summary.count}; floor ${summary.minimum.toFixed(2)}%)`;
    console.log(result);
    if (summary.percent < summary.minimum) {
      failures.push(result);
    }
  }
  if (failures.length > 0) {
    throw new Error(`Rust line coverage floor not met:\n${failures.join('\n')}`);
  }
  return summaries;
}

function main() {
  const reportPath = process.argv[2];
  if (!reportPath) {
    throw new Error('Usage: node scripts/rust-coverage.mjs <llvm-coverage.json>');
  }
  enforceRustCoverage(JSON.parse(readFileSync(reportPath, 'utf8')));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
