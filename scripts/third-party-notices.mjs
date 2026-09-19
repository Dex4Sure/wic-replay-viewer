#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outputPath = resolve(repoRoot, 'THIRD_PARTY_NOTICES.txt');
const overridesRoot = resolve(repoRoot, 'third-party');
const overrides = JSON.parse(
  readFileSync(resolve(overridesRoot, 'license-overrides.json'), 'utf8'),
);
const cratesIoSource = 'registry+https://github.com/rust-lang/crates.io-index';
const approvedLicenseExpressions = new Set([
  '(MIT OR Apache-2.0) AND Unicode-3.0',
  '0BSD OR MIT OR Apache-2.0',
  'Apache-2.0',
  'Apache-2.0 / MIT',
  'Apache-2.0 AND MIT',
  'Apache-2.0 OR MIT',
  'Apache-2.0 WITH LLVM-exception',
  'Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT',
  'Apache-2.0/MIT',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'BSD-3-Clause AND MIT',
  'BSD-3-Clause OR MIT OR Apache-2.0',
  'BSD-3-Clause/MIT',
  'CC0-1.0 OR MIT-0 OR Apache-2.0',
  'ISC',
  'MIT',
  'MIT OR Apache-2.0',
  'MIT OR Apache-2.0 OR LGPL-2.1-or-later',
  'MIT OR Apache-2.0 OR Zlib',
  'MIT OR Zlib OR Apache-2.0',
  'MIT/Apache-2.0',
  'MPL-2.0',
  'Unicode-3.0',
  'Unlicense OR MIT',
  'Unlicense/MIT',
  'Zlib',
  'Zlib OR Apache-2.0 OR MIT',
]);
const primaryNoticeName = /^(?:licen[cs]e|copying|notice|copyright|thirdpartynotice)/i;
const noticeName = /^(?:authors|licen[cs]e|copying|notice|copyright|thirdpartynotice)/i;

function fail(message) {
  throw new Error(message);
}

function normalizeText(value) {
  const lines = value.replaceAll('\r\n', '\n').split('\n');
  return `${lines
    .map((line) => line.trimEnd())
    .join('\n')
    .trimEnd()}\n`;
}

function oneLine(value) {
  return value.replaceAll(/\s+/g, ' ').trim();
}

function tomlString(contents, key) {
  return new RegExp(`^${key} = "([^"]+)"$`, 'm').exec(contents)?.[1];
}

function tarString(buffer, offset, length) {
  const field = buffer.subarray(offset, offset + length);
  const end = field.indexOf(0);
  return field.subarray(0, end === -1 ? field.length : end).toString('utf8');
}

function tarFiles(compressed) {
  const archive = gunzipSync(compressed);
  const files = new Map();
  let offset = 0;
  while (offset + 512 <= archive.length) {
    const header = archive.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      break;
    }
    const name = tarString(header, 0, 100);
    const prefix = tarString(header, 345, 155);
    const path = prefix === '' ? name : `${prefix}/${name}`;
    const sizeText = tarString(header, 124, 12).trim();
    const size = Number.parseInt(sizeText || '0', 8);
    if (!Number.isSafeInteger(size) || size < 0) {
      fail(`Invalid tar entry size for ${path}.`);
    }
    const type = String.fromCharCode(header[156]);
    const contentsOffset = offset + 512;
    if (type === '\0' || type === '0') {
      files.set(path, archive.subarray(contentsOffset, contentsOffset + size));
    }
    offset = contentsOffset + Math.ceil(size / 512) * 512;
  }
  return files;
}

function cargoPackageArchive(name, version, checksum) {
  const cargoHome = process.env.CARGO_HOME ?? resolve(homedir(), '.cargo');
  const registryCache = resolve(cargoHome, 'registry', 'cache');
  if (!existsSync(registryCache)) {
    fail(`Cargo registry archives are unavailable; run cargo fetch --locked first.`);
  }
  const archiveName = `${name}-${version}.crate`;
  const mismatches = [];
  for (const registry of readdirSync(registryCache, { withFileTypes: true })) {
    if (registry.isDirectory()) {
      const candidate = resolve(registryCache, registry.name, archiveName);
      if (existsSync(candidate)) {
        const archive = readFileSync(candidate);
        const actual = createHash('sha256').update(archive).digest('hex');
        if (actual === checksum) {
          return tarFiles(archive);
        }
        mismatches.push(actual);
      }
    }
  }
  if (mismatches.length > 0) {
    fail(`${name}@${version} Cargo archive checksum mismatch: ${mismatches.join(', ')}.`);
  }
  fail(`Cargo archive is unavailable for ${name}@${version}; run cargo fetch --locked first.`);
}

function cargoComponents() {
  const lock = readFileSync(resolve(repoRoot, 'Cargo.lock'), 'utf8');
  return lock
    .split('[[package]]')
    .slice(1)
    .map((block) => ({
      name: tomlString(block, 'name'),
      checksum: tomlString(block, 'checksum'),
      source: tomlString(block, 'source'),
      version: tomlString(block, 'version'),
    }))
    .filter((item) => item.source !== undefined)
    .map((item) => {
      if (item.source !== cratesIoSource) {
        fail(
          `${item.name}@${item.version} uses unreviewed Cargo source ${JSON.stringify(item.source)}.`,
        );
      }
      if (!/^[0-9a-f]{64}$/.test(item.checksum ?? '')) {
        fail(`${item.name}@${item.version} has no valid Cargo.lock checksum.`);
      }
      const archiveFiles = cargoPackageArchive(item.name, item.version, item.checksum);
      const archiveRoot = `${item.name}-${item.version}`;
      const manifestBuffer = archiveFiles.get(`${archiveRoot}/Cargo.toml`);
      if (manifestBuffer === undefined) {
        fail(`${item.name}@${item.version} archive has no Cargo.toml.`);
      }
      const manifest = manifestBuffer.toString('utf8');
      const packageHeading = /^\[package\]\r?\n/m.exec(manifest);
      if (packageHeading === null) {
        fail(`${item.name}@${item.version} has no [package] metadata.`);
      }
      const packageRemainder = manifest.slice(packageHeading.index + packageHeading[0].length);
      const nextSection = packageRemainder.search(/\r?\n\[/);
      const packageSection =
        nextSection === -1 ? packageRemainder : packageRemainder.slice(0, nextSection);
      const authorsBlock = /^authors = \[([\s\S]*?)\]$/m.exec(packageSection)?.[1] ?? '';
      const authors = [...authorsBlock.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
      return {
        authors,
        archiveFiles,
        archiveRoot,
        checksum: item.checksum,
        ecosystem: 'Rust',
        license: tomlString(packageSection, 'license'),
        name: item.name,
        source: `https://crates.io/crates/${item.name}/${item.version}`,
        version: item.version,
      };
    });
}

function npmComponents() {
  const lock = JSON.parse(readFileSync(resolve(repoRoot, 'package-lock.json'), 'utf8'));
  const installedLockPath = resolve(repoRoot, 'node_modules', '.package-lock.json');
  if (!existsSync(installedLockPath)) {
    fail('Installed npm lock metadata is unavailable; run npm ci first.');
  }
  const installedLock = JSON.parse(readFileSync(installedLockPath, 'utf8'));
  return Object.entries(lock.packages)
    .filter(([location, item]) => location !== '' && item.dev !== true)
    .map(([location, item]) => {
      const installed = installedLock.packages?.[location];
      if (
        installed?.version !== item.version ||
        (item.integrity !== undefined && installed.integrity !== item.integrity)
      ) {
        fail(`Installed npm package ${location} does not match package-lock.json; run npm ci.`);
      }
      const packageJsonPath = resolve(repoRoot, location, 'package.json');
      const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
      if (packageJson.version !== item.version) {
        fail(`Installed npm package ${location} has unexpected version ${packageJson.version}.`);
      }
      return {
        authors: [
          typeof packageJson.author === 'string'
            ? packageJson.author
            : (packageJson.author?.name ?? ''),
        ],
        directory: resolve(repoRoot, location),
        ecosystem: 'npm',
        license: item.license ?? packageJson.license,
        name: packageJson.name,
        source: `https://www.npmjs.com/package/${packageJson.name}/v/${item.version}`,
        version: item.version,
      };
    });
}

function componentKey(component) {
  return `${component.ecosystem}:${component.name}@${component.version}`;
}

function noticeDocuments(component) {
  if (component.archiveFiles !== undefined) {
    const prefix = `${component.archiveRoot}/`;
    return [...component.archiveFiles.entries()]
      .filter(([path]) => {
        const relative = path.startsWith(prefix) ? path.slice(prefix.length) : path;
        return !relative.includes('/') && noticeName.test(relative);
      })
      .map(([path, contents]) => ({
        name: path.slice(prefix.length),
        text: normalizeText(contents.toString('utf8')),
      }))
      .sort((left, right) => left.name.localeCompare(right.name));
  }
  if (!existsSync(component.directory)) {
    fail(`Dependency source is unavailable for ${componentKey(component)}.`);
  }
  return readdirSync(component.directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && noticeName.test(entry.name))
    .map((entry) => ({
      name: entry.name,
      text: normalizeText(readFileSync(resolve(component.directory, entry.name), 'utf8')),
    }))
    .sort((left, right) => left.name.localeCompare(right.name));
}

function overrideDocuments(component, usedOverrides) {
  const key = componentKey(component);
  const override = overrides.components[key];
  if (override === undefined) {
    fail(`${key} has no standalone notice document or reviewed upstream fallback.`);
  }
  const [expectedChecksum, documentSetName] = override;
  if (component.checksum !== expectedChecksum) {
    fail(`${key} upstream notice fallback does not match its Cargo.lock checksum.`);
  }
  const documentSet = overrides.documentSets[documentSetName];
  if (!Array.isArray(documentSet) || documentSet.length === 0) {
    fail(`${key} refers to missing upstream notice set ${documentSetName}.`);
  }
  usedOverrides.add(key);
  return documentSet.map((document) => {
    const path = resolve(overridesRoot, document.path);
    const contents = readFileSync(path);
    const actual = createHash('sha256').update(contents).digest('hex');
    if (actual !== document.sha256) {
      fail(`${document.path} checksum mismatch; expected ${document.sha256}, got ${actual}.`);
    }
    return {
      name: `${document.path} (from ${document.source})`,
      text: normalizeText(contents.toString('utf8')),
    };
  });
}

function hasLicenseTerms(documents) {
  return documents.some(
    (document) =>
      primaryNoticeName.test(document.name) ||
      (/^authors$/i.test(document.name) &&
        /permission is hereby granted|licensed under the apache license|gnu lesser general public license/i.test(
          document.text,
        )),
  );
}

function validateComponents(components) {
  const errors = [];
  for (const component of components) {
    if (typeof component.license !== 'string' || component.license.trim() === '') {
      errors.push(`${componentKey(component)} has no declared license.`);
    } else if (!approvedLicenseExpressions.has(component.license)) {
      errors.push(
        `${componentKey(component)} uses unreviewed license expression ${JSON.stringify(component.license)}.`,
      );
    }
  }
  if (errors.length > 0) {
    fail(`Third-party license review required:\n- ${errors.join('\n- ')}`);
  }
}

function renderNotices() {
  const byKey = new Map();
  for (const component of [...npmComponents(), ...cargoComponents()]) {
    byKey.set(componentKey(component), component);
  }
  const components = [...byKey.values()].sort((left, right) =>
    componentKey(left).localeCompare(componentKey(right)),
  );
  validateComponents(components);

  const documentGroups = new Map();
  const usedOverrides = new Set();
  for (const component of components) {
    let documents = noticeDocuments(component);
    if (!hasLicenseTerms(documents)) {
      documents = [...documents, ...overrideDocuments(component, usedOverrides)];
    }
    for (const document of documents) {
      const hash = createHash('sha256').update(document.text).digest('hex');
      const group = documentGroups.get(hash) ?? {
        components: [],
        names: new Set(),
        text: document.text,
      };
      group.components.push(componentKey(component));
      group.names.add(document.name);
      documentGroups.set(hash, group);
    }
  }
  const staleOverrides = Object.keys(overrides.components).filter((key) => !usedOverrides.has(key));
  if (staleOverrides.length > 0) {
    fail(
      `Unused upstream notice overrides must be reviewed or removed:\n- ${staleOverrides.join('\n- ')}`,
    );
  }

  const lines = [
    'WiC Replay Viewer - Third-Party Notices',
    '',
    'This file is generated from Cargo.lock, package-lock.json, checksum-verified Cargo',
    'crate archives, an npm tree installed by npm ci, and reviewed commit-pinned upstream',
    'documents for incomplete crate archives. Do not edit it by hand.',
    'When an upstream crate offers alternative licences, a reviewed fallback may record',
    'the permissive MIT option used for this distribution.',
    '',
    'The WiC Replay Viewer source code is licensed separately under the MIT License.',
    'The following notices cover the complete locked dependency set. This inventory is',
    'intentionally conservative: build-only, development, and platform-specific packages',
    'may be listed even when their code is absent from a particular release artifact.',
    '',
    'For every MPL-2.0 component, the exact corresponding source archive is available',
    'from the versioned crates.io Source URL shown in the inventory. These dependencies',
    'are used without source modifications by WiC Replay Viewer.',
    '',
    `Dependency components: ${components.length}`,
    '',
    'DEPENDENCY INVENTORY',
    '====================',
    '',
  ];
  for (const component of components) {
    const authors = component.authors.map(oneLine).filter(Boolean).join('; ') || 'not declared';
    lines.push(`${componentKey(component)}`);
    lines.push(`  License: ${component.license}`);
    lines.push(`  Authors: ${authors}`);
    lines.push(`  Source: ${component.source}`);
  }

  lines.push('');
  lines.push('LICENSE AND NOTICE DOCUMENTS');
  lines.push('============================');
  for (const [hash, group] of [...documentGroups.entries()].sort((left, right) =>
    left[0].localeCompare(right[0]),
  )) {
    lines.push('');
    lines.push('--------------------------------------------------------------------------------');
    lines.push(`Document SHA-256: ${hash}`);
    lines.push(`Source filenames: ${[...group.names].sort().join(', ')}`);
    lines.push('Applies to:');
    for (const component of group.components.sort()) {
      lines.push(`  ${component}`);
    }
    lines.push('--------------------------------------------------------------------------------');
    lines.push(group.text.trimEnd());
  }
  return `${lines.join('\n')}\n`;
}

function main(argument) {
  if (!['check', 'generate'].includes(argument)) {
    fail('Usage: node scripts/third-party-notices.mjs {check|generate}');
  }
  const rendered = renderNotices();
  if (argument === 'generate') {
    writeFileSync(outputPath, rendered);
    console.log(`Generated THIRD_PARTY_NOTICES.txt (${rendered.length} bytes).`);
    return;
  }
  if (!existsSync(outputPath)) {
    fail('THIRD_PARTY_NOTICES.txt is missing; run npm run licenses:generate.');
  }
  const tracked = readFileSync(outputPath, 'utf8');
  if (tracked !== rendered) {
    fail('THIRD_PARTY_NOTICES.txt is stale; run npm run licenses:generate and review it.');
  }
  console.log('Third-party notices match the locked dependency graph.');
}

try {
  main(process.argv[2]);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
