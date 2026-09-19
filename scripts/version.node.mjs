import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  bumpVersion,
  prepareReleaseContents,
  releaseNotes,
  validateVersionContents,
} from './version.mjs';

const productFiles = [
  'package.json',
  'package-lock.json',
  'Cargo.toml',
  'Cargo.lock',
  'src-tauri/Cargo.toml',
  'src-tauri/tauri.conf.json',
  'parser/rust_parser/Cargo.toml',
  'CHANGELOG.md',
];
const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const files = () => Object.fromEntries(productFiles.map((path) => [path, read(path)]));

test('semantic release types calculate deterministic next versions', () => {
  assert.equal(bumpVersion('0.2.0', 'patch'), '0.2.1');
  assert.equal(bumpVersion('0.2.0', 'minor'), '0.3.0');
  assert.equal(bumpVersion('0.2.0', 'major'), '1.0.0');
  assert.throws(() => bumpVersion('0.2', 'minor'), /major\.minor\.patch/);
  assert.throws(() => bumpVersion('0.2.0', 'banana'), /patch, minor, or major/);
});

test('current product metadata is synchronized', () => {
  const current = files();
  const result = validateVersionContents(current);
  assert.deepEqual(result.errors, []);
  assert.equal(result.version, JSON.parse(current['package.json']).version);
});

test('release preparation updates every product version without touching parser lineage', () => {
  const original = files();
  original['CHANGELOG.md'] = original['CHANGELOG.md'].replace(
    '## Unreleased',
    '## Unreleased\n\n### Highlights\n\n- Exercise release preparation.\n\n### Added\n\n- Retain technical detail.',
  );
  const currentVersion = JSON.parse(original['package.json']).version;
  const nextVersion = bumpVersion(currentVersion, 'minor');
  const prepared = prepareReleaseContents(original, 'minor', '2026-08-31');
  const result = validateVersionContents(prepared.files, `v${nextVersion}`);

  assert.deepEqual(result.errors, []);
  assert.equal(prepared.currentVersion, currentVersion);
  assert.equal(prepared.nextVersion, nextVersion);
  assert.match(
    prepared.files['CHANGELOG.md'],
    new RegExp(`^## ${nextVersion.replaceAll('.', '\\.')} - 2026-08-31$`, 'm'),
  );
  assert.equal(
    prepared.files['parser/rust_parser/Cargo.toml'],
    original['parser/rust_parser/Cargo.toml'],
  );
  assert.equal(
    releaseNotes(prepared.files['CHANGELOG.md'], nextVersion),
    '### Highlights\n\n- Exercise release preparation.',
  );
});

test('a mismatched release tag is rejected', () => {
  const current = files();
  const version = JSON.parse(current['package.json']).version;
  const result = validateVersionContents(current, 'v999.0.0');
  assert.match(result.errors.join('\n'), new RegExp(`expected v${version.replaceAll('.', '\\.')}`));
});

test('public release notes require a short Highlights section', () => {
  assert.throws(
    () => releaseNotes('## 1.2.3 - 2026-09-13\n\n### Added\n\n- Technical detail.', '1.2.3'),
    /has no Highlights section/,
  );
});
