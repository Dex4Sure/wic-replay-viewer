#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
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
const writableReleaseFiles = [
  'package.json',
  'package-lock.json',
  'Cargo.toml',
  'Cargo.lock',
  'src-tauri/Cargo.toml',
  'CHANGELOG.md',
];
const semverPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function fail(message) {
  throw new Error(message);
}

function parseJson(contents, path) {
  try {
    return JSON.parse(contents);
  } catch (error) {
    fail(`${path} is not valid JSON: ${error.message}`);
  }
}

function parseSemver(version, label = 'version') {
  const match = semverPattern.exec(version);
  if (match === null) {
    fail(`${label} must be a stable major.minor.patch version, got ${JSON.stringify(version)}.`);
  }
  return match.slice(1).map(Number);
}

export function bumpVersion(version, releaseType) {
  const [major, minor, patch] = parseSemver(version);
  switch (releaseType) {
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'major':
      return `${major + 1}.0.0`;
    default:
      fail(`Release type must be patch, minor, or major, got ${JSON.stringify(releaseType)}.`);
  }
}

function cargoPackageVersion(contents, path) {
  const section = /^\[package\]\r?\n([\s\S]*?)(?=\r?\n\[|$)/.exec(contents);
  const version = section?.[1].match(/^version = "([^"]+)"$/m)?.[1];
  if (version === undefined) {
    fail(`Cannot find [package] version in ${path}.`);
  }
  return version;
}

function replaceCargoPackageVersion(contents, path, version) {
  const current = cargoPackageVersion(contents, path);
  return contents.replace(`version = "${current}"`, `version = "${version}"`);
}

function cargoLockPackageVersion(contents, packageName) {
  const escapedName = packageName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const expression = new RegExp(
    `\\[\\[package\\]\\]\\nname = "${escapedName}"\\nversion = "([^"]+)"`,
    'g',
  );
  const matches = [...contents.matchAll(expression)];
  if (matches.length !== 1) {
    fail(`Expected exactly one ${packageName} package in Cargo.lock, found ${matches.length}.`);
  }
  return matches[0][1];
}

function replaceCargoLockPackageVersion(contents, packageName, version) {
  const current = cargoLockPackageVersion(contents, packageName);
  return contents.replace(
    `name = "${packageName}"\nversion = "${current}"`,
    `name = "${packageName}"\nversion = "${version}"`,
  );
}

function newestChangelogVersion(contents) {
  return /^## (\d+\.\d+\.\d+) - \d{4}-\d{2}-\d{2}$/m.exec(contents)?.[1];
}

function readProductFiles() {
  return Object.fromEntries(
    productFiles.map((path) => [path, readFileSync(resolve(repoRoot, path), 'utf8')]),
  );
}

export function validateVersionContents(files, expectedTag) {
  const errors = [];
  const packageJson = parseJson(files['package.json'], 'package.json');
  const packageLock = parseJson(files['package-lock.json'], 'package-lock.json');
  const tauriConfig = parseJson(files['src-tauri/tauri.conf.json'], 'src-tauri/tauri.conf.json');
  const version = packageJson.version;

  try {
    parseSemver(version, 'package.json version');
  } catch (error) {
    errors.push(error.message);
  }

  const comparisons = [
    ['package-lock.json top-level version', packageLock.version],
    ['package-lock.json workspace version', packageLock.packages?.['']?.version],
    ['Cargo.toml package version', cargoPackageVersion(files['Cargo.toml'], 'Cargo.toml')],
    [
      'src-tauri/Cargo.toml package version',
      cargoPackageVersion(files['src-tauri/Cargo.toml'], 'src-tauri/Cargo.toml'),
    ],
    [
      'Cargo.lock wic-replay-viewer version',
      cargoLockPackageVersion(files['Cargo.lock'], 'wic-replay-viewer'),
    ],
    [
      'Cargo.lock wic-replay-viewer-app version',
      cargoLockPackageVersion(files['Cargo.lock'], 'wic-replay-viewer-app'),
    ],
  ];
  for (const [label, actual] of comparisons) {
    if (actual !== version) {
      errors.push(`${label} is ${JSON.stringify(actual)}; expected ${JSON.stringify(version)}.`);
    }
  }

  if (tauriConfig.version !== '../package.json') {
    errors.push(
      `src-tauri/tauri.conf.json version must reference "../package.json", got ${JSON.stringify(tauriConfig.version)}.`,
    );
  }

  const rootCargo = files['Cargo.toml'];
  const parserDependency = /^wic_replay_parser = \{([^\n]+)\}$/m.exec(rootCargo)?.[1];
  if (parserDependency === undefined || !/\bpath = "parser\/rust_parser"/.test(parserDependency)) {
    errors.push('Cargo.toml must use the workspace parser through its local path.');
  } else if (/\bversion\s*=/.test(parserDependency)) {
    errors.push(
      'Cargo.toml local parser dependency must not couple to its historical crate version.',
    );
  }

  const parserCargo = files['parser/rust_parser/Cargo.toml'];
  if (!/^publish = false$/m.test(parserCargo)) {
    errors.push(
      'parser/rust_parser/Cargo.toml must mark the historical parser crate publish = false.',
    );
  }

  const changelogVersion = newestChangelogVersion(files['CHANGELOG.md']);
  if (changelogVersion !== undefined && changelogVersion !== version) {
    errors.push(
      `Newest CHANGELOG.md release is ${changelogVersion}; expected product version ${version}.`,
    );
  }

  if (expectedTag !== undefined && expectedTag !== `v${version}`) {
    errors.push(`Release tag is ${expectedTag}; expected v${version}.`);
  }

  return { errors, version };
}

function assertVersionContents(files, expectedTag) {
  const result = validateVersionContents(files, expectedTag);
  if (result.errors.length > 0) {
    fail(`Version consistency check failed:\n- ${result.errors.join('\n- ')}`);
  }
  return result.version;
}

function releaseDateToday() {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, '0');
  const day = String(now.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function validateReleaseDate(date) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || Number.isNaN(Date.parse(`${date}T00:00:00Z`))) {
    fail(`Release date must be YYYY-MM-DD, got ${JSON.stringify(date)}.`);
  }
}

function releaseChangelog(contents, version, date) {
  const marker = '## Unreleased';
  const markerIndex = contents.indexOf(marker);
  if (markerIndex === -1) {
    fail('CHANGELOG.md has no Unreleased section.');
  }
  const bodyStart = markerIndex + marker.length;
  const nextRelease = contents.indexOf('\n## ', bodyStart);
  const body = contents.slice(bodyStart, nextRelease === -1 ? undefined : nextRelease);
  if (!/^\s*### /m.test(body)) {
    fail('CHANGELOG.md Unreleased section is empty; refusing to prepare an empty release.');
  }
  return `${contents.slice(0, bodyStart)}\n\n## ${version} - ${date}${contents.slice(bodyStart)}`;
}

export function releaseNotes(contents, version) {
  parseSemver(version, 'release-notes version');
  const heading = new RegExp(`^## ${version.replaceAll('.', '\\.')} - \\d{4}-\\d{2}-\\d{2}$`, 'm');
  const match = heading.exec(contents);
  if (match === null) {
    fail(`CHANGELOG.md has no release section for ${version}.`);
  }
  const bodyStart = match.index + match[0].length;
  const nextRelease = contents.indexOf('\n## ', bodyStart);
  const notes = contents.slice(bodyStart, nextRelease === -1 ? undefined : nextRelease).trim();
  if (notes === '') {
    fail(`CHANGELOG.md release section for ${version} is empty.`);
  }
  const highlightsHeading = /^### Highlights$/m.exec(notes);
  if (highlightsHeading === null) {
    fail(`CHANGELOG.md release section for ${version} has no Highlights section.`);
  }
  const highlightsStart = highlightsHeading.index;
  const nextSection = notes.indexOf('\n### ', highlightsStart + highlightsHeading[0].length);
  return notes.slice(highlightsStart, nextSection === -1 ? undefined : nextSection).trim();
}

export function prepareReleaseContents(files, releaseType, date) {
  validateReleaseDate(date);
  const currentVersion = assertVersionContents(files);
  const nextVersion = bumpVersion(currentVersion, releaseType);
  const next = { ...files };

  const packageJson = parseJson(next['package.json'], 'package.json');
  packageJson.version = nextVersion;
  next['package.json'] = `${JSON.stringify(packageJson, null, 2)}\n`;

  const packageLock = parseJson(next['package-lock.json'], 'package-lock.json');
  packageLock.version = nextVersion;
  packageLock.packages[''].version = nextVersion;
  next['package-lock.json'] = `${JSON.stringify(packageLock, null, 2)}\n`;

  next['Cargo.toml'] = replaceCargoPackageVersion(next['Cargo.toml'], 'Cargo.toml', nextVersion);
  next['src-tauri/Cargo.toml'] = replaceCargoPackageVersion(
    next['src-tauri/Cargo.toml'],
    'src-tauri/Cargo.toml',
    nextVersion,
  );
  next['Cargo.lock'] = replaceCargoLockPackageVersion(
    replaceCargoLockPackageVersion(next['Cargo.lock'], 'wic-replay-viewer', nextVersion),
    'wic-replay-viewer-app',
    nextVersion,
  );

  next['CHANGELOG.md'] = releaseChangelog(next['CHANGELOG.md'], nextVersion, date);

  assertVersionContents(next);
  return { files: next, currentVersion, nextVersion };
}

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, stdio: 'inherit' });
  if (result.error !== undefined) {
    fail(`Failed to run ${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`${command} exited with status ${result.status}.`);
  }
}

function parseCheckArguments(args) {
  if (args.length === 0) {
    return undefined;
  }
  if (args.length === 2 && args[0] === '--tag') {
    return args[1];
  }
  fail('Usage: npm run version:check -- [--tag vMAJOR.MINOR.PATCH]');
}

function parsePrepareArguments(args) {
  const releaseType = args.find((argument) => ['patch', 'minor', 'major'].includes(argument));
  const dryRun = args.includes('--dry-run');
  const unknown = args.filter(
    (argument) => !['patch', 'minor', 'major', '--dry-run'].includes(argument),
  );
  if (
    releaseType === undefined ||
    unknown.length > 0 ||
    args.filter((argument) => argument !== '--dry-run').length !== 1
  ) {
    fail('Usage: npm run release:prepare -- {patch|minor|major} [--dry-run]');
  }
  return { dryRun, releaseType };
}

function main(args) {
  const [command, ...commandArgs] = args;
  if (command === 'check') {
    const tag = parseCheckArguments(commandArgs);
    const version = assertVersionContents(readProductFiles(), tag);
    console.log(
      `Version ${version} is synchronized${tag === undefined ? '.' : ` with tag ${tag}.`}`,
    );
    return;
  }
  if (command === 'notes') {
    if (commandArgs.length !== 0) {
      fail('Usage: npm run release:notes');
    }
    const files = readProductFiles();
    const version = assertVersionContents(files);
    console.log(releaseNotes(files['CHANGELOG.md'], version));
    return;
  }
  if (command === 'prepare') {
    const { dryRun, releaseType } = parsePrepareArguments(commandArgs);
    if (!dryRun && process.env.WIC_RELEASE_CHECKOUT_VERIFIED !== '1') {
      fail('Run release preparation through npm run release:prepare so Git safeguards execute.');
    }
    const originals = readProductFiles();
    const date = releaseDateToday();
    const prepared = prepareReleaseContents(originals, releaseType, date);
    if (dryRun) {
      console.log(
        `Dry run: ${releaseType} would prepare ${prepared.currentVersion} -> ${prepared.nextVersion} (${date}).`,
      );
      return;
    }

    try {
      for (const path of writableReleaseFiles) {
        writeFileSync(resolve(repoRoot, path), prepared.files[path]);
      }
      run('npm', ['run', 'version:check']);
      run('./scripts/quality.sh', ['full']);
    } catch (error) {
      for (const path of writableReleaseFiles) {
        writeFileSync(resolve(repoRoot, path), originals[path]);
      }
      console.error('Release preparation failed; restored all versioned files.');
      throw error;
    }

    console.log(`Prepared ${prepared.currentVersion} -> ${prepared.nextVersion}.`);
    console.log('Review the diff, then commit and tag explicitly:');
    console.log(`  git commit -am "Release v${prepared.nextVersion}"`);
    console.log(
      `  git tag -a v${prepared.nextVersion} -m "WiC Replay Viewer ${prepared.nextVersion}"`,
    );
    console.log(`  git push origin main v${prepared.nextVersion}`);
    return;
  }
  fail('Usage: node scripts/version.mjs {check|notes|prepare}');
}

const invokedPath = process.argv[1] === undefined ? undefined : resolve(process.argv[1]);
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
