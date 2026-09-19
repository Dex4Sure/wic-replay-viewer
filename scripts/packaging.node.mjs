import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import viteConfig from '../vite.config.ts';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('Linux defaults to AppImage packaging', () => {
  const config = JSON.parse(read('src-tauri/tauri.linux.conf.json'));
  const packageJson = JSON.parse(read('package.json'));
  assert.equal(config.bundle.active, true);
  assert.deepEqual(config.bundle.targets, ['appimage']);
  assert.equal(packageJson.scripts['build:linux'], './scripts/build-appimage.sh');
  assert.equal(packageJson.scripts['build:appimage'], packageJson.scripts['build:linux']);
  assert.equal(packageJson.scripts['build:flatpak'], undefined);
});

test('Windows disables Tauri installers in favor of the portable executable', () => {
  const config = JSON.parse(read('src-tauri/tauri.windows.conf.json'));
  const baseConfig = JSON.parse(read('src-tauri/tauri.conf.json'));
  const packageJson = JSON.parse(read('package.json'));
  const workflow = read('.github/workflows/build-desktop-artifacts.yml');
  assert.equal(config.bundle.active, false);
  assert.equal(baseConfig.bundle.windows, undefined);
  assert.match(packageJson.scripts['build:windows'], /--no-bundle/);
  assert.doesNotMatch(packageJson.scripts['build:windows'], /nsis|msi/);
  assert.match(
    workflow,
    /Compress-Archive .*wic-replay-viewer\.exe, LICENSE, NOTICE\.txt, THIRD_PARTY_NOTICES\.txt/,
  );
});

test('macOS remains a local ad-hoc-signed build without hosted release automation', () => {
  const config = JSON.parse(read('src-tauri/tauri.conf.json'));
  const workflow = read('.github/workflows/build-desktop-artifacts.yml');
  assert.equal(config.bundle.macOS.signingIdentity, '-');
  assert.doesNotMatch(workflow, /runs-on: macos-|aarch64-apple-darwin|\.dmg/);
  assert.doesNotMatch(workflow, /APPLE_(CERTIFICATE|CERTIFICATE_PASSWORD|ID|PASSWORD|TEAM_ID)/);
});

test('release packages retain the MIT license and Dex4Sure attribution', () => {
  const packageJson = JSON.parse(read('package.json'));
  const tauriConfig = JSON.parse(read('src-tauri/tauri.conf.json'));
  const rootCargo = read('Cargo.toml');
  const appCargo = read('src-tauri/Cargo.toml');
  const parserCargo = read('parser/rust_parser/Cargo.toml');
  const license = read('LICENSE');
  const notice = read('NOTICE.txt');
  const thirdPartyNotices = read('THIRD_PARTY_NOTICES.txt');
  const workflow = read('.github/workflows/build-desktop-artifacts.yml');

  assert.equal(packageJson.author, 'Dex4Sure');
  assert.equal(packageJson.license, 'MIT');
  assert.equal(tauriConfig.bundle.copyright, 'Copyright (c) 2026 Dex4Sure');
  assert.equal(tauriConfig.bundle.license, 'MIT');
  assert.equal(tauriConfig.bundle.licenseFile, '../LICENSE');
  assert.equal(tauriConfig.bundle.resources['../LICENSE'], 'LICENSE');
  assert.equal(tauriConfig.bundle.resources['../NOTICE.txt'], 'NOTICE.txt');
  assert.equal(
    tauriConfig.bundle.resources['../THIRD_PARTY_NOTICES.txt'],
    'THIRD_PARTY_NOTICES.txt',
  );
  for (const cargoManifest of [rootCargo, appCargo, parserCargo]) {
    assert.match(cargoManifest, /^authors = \["Dex4Sure"\]$/m);
    assert.match(cargoManifest, /^license = "MIT"$/m);
  }
  assert.match(license, /^MIT License$/m);
  assert.match(license, /^Copyright \(c\) 2026 Dex4Sure$/m);
  assert.match(notice, /^WiC Replay Viewer$/m);
  assert.match(notice, /not affiliated\s+with or endorsed by Ubisoft or Massive Entertainment/);
  assert.match(thirdPartyNotices, /^WiC Replay Viewer - Third-Party Notices$/m);
  assert.match(thirdPartyNotices, /^Rust:cssparser@0\.36\.0$/m);
  assert.match(thirdPartyNotices, /^npm:vue@3\.5\.42$/m);
  assert.match(
    workflow,
    /Compress-Archive .*wic-replay-viewer\.exe, LICENSE, NOTICE\.txt, THIRD_PARTY_NOTICES\.txt/,
  );
});

test('AppImage builds embed the frontend and retain diagnostic symbols separately', () => {
  const script = read('scripts/build-appimage.sh');
  assert.match(script, /tauri\.mjs build --ci --no-bundle -- --locked/);
  assert.match(script, /objcopy --only-keep-debug/);
  assert.match(script, /diagnostic-symbols\/linux\/wic-replay-viewer\.debug/);
  assert.match(script, /tauri\.mjs bundle --ci --bundles appimage/);
  assert.match(script, /APPIMAGE_EXTRACT_AND_RUN=1/);
  assert.match(script, /scripts\/package-appimage\.sh/);
  assert.match(read('src-tauri/Cargo.toml'), /^custom-protocol = \["tauri\/custom-protocol"\]$/m);
});

test(
  'AppImage packaging excludes the conflicting Wayland client and rejects partial output',
  { skip: process.platform !== 'linux' },
  () => {
    const root = mkdtempSync(join(tmpdir(), 'wic-appimage-package-'));
    try {
      const appDir = join(root, 'Viewer.AppDir');
      const libraryDir = join(appDir, 'usr/lib');
      const cache = join(root, 'cache');
      const tool = join(cache, 'tauri/linuxdeploy-plugin-appimage.AppImage');
      const output = join(root, 'Viewer.AppImage');
      mkdirSync(libraryDir, { recursive: true });
      mkdirSync(join(cache, 'tauri'), { recursive: true });
      writeFileSync(
        join(appDir, 'AppRun'),
        `#!/bin/sh
printf '%s\\n' "$APPIMAGE_GTK_THEME" "$@"
`,
        { mode: 0o755 },
      );
      writeFileSync(join(libraryDir, 'libwayland-client.so.0'), 'old client');
      writeFileSync(join(libraryDir, 'libwayland-client.so.0.20.0'), 'old client');
      writeFileSync(join(libraryDir, 'libwebkit2gtk-4.1.so.0'), 'bundled webview');
      writeFileSync(tool, '#!/bin/sh\nprintf packaged > "$OUTPUT"\n', { mode: 0o755 });
      const run = () =>
        spawnSync(
          'bash',
          [fileURLToPath(new URL('./package-appimage.sh', import.meta.url)), appDir, output],
          { env: { ...process.env, XDG_CACHE_HOME: cache }, encoding: 'utf8' },
        );
      let result = run();
      assert.equal(result.status, 0, result.stderr);
      assert.equal(readFileSync(output, 'utf8'), 'packaged');
      assert.equal(existsSync(join(libraryDir, 'libwayland-client.so.0')), false);
      assert.equal(existsSync(join(libraryDir, 'libwayland-client.so.0.20.0')), false);
      assert.equal(
        readFileSync(join(libraryDir, 'libwebkit2gtk-4.1.so.0'), 'utf8'),
        'bundled webview',
      );
      const launch = (overrides = {}) =>
        spawnSync(join(appDir, 'AppRun'), ['argument with spaces'], {
          env: { ...process.env, GTK_THEME: '', APPIMAGE_GTK_THEME: '', ...overrides },
          encoding: 'utf8',
        });
      const launched = launch();
      assert.equal(launched.status, 0, launched.stderr);
      assert.equal(launched.stdout, 'Adwaita:dark\nargument with spaces\n');
      assert.equal(
        launch({ GTK_THEME: 'Adwaita:light' }).stdout,
        'Adwaita:light\nargument with spaces\n',
      );
      assert.equal(
        launch({ GTK_THEME: 'Adwaita:light', APPIMAGE_GTK_THEME: 'Adwaita:dark' }).stdout,
        'Adwaita:dark\nargument with spaces\n',
      );
      result = run();
      assert.equal(result.status, 0, result.stderr);
      assert.equal(launch().stdout, 'Adwaita:dark\nargument with spaces\n');
      writeFileSync(tool, '#!/bin/sh\nprintf partial > "$OUTPUT"\nexit 7\n');
      result = run();
      assert.equal(result.status, 7);
      assert.equal(existsSync(output), false);
      assert.equal(existsSync(`${output}.tmp.AppImage`), false);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  },
);

test('development discovery is bounded at every scanner', () => {
  const styles = read('frontend/styles.css');
  const tauriIgnore = read('.taurignore');

  // Wine prefixes under target contain a z: symlink to the host root. Vite's
  // optimizer, Vite's watcher, and Tauri's watcher have independent discovery
  // rules, so every layer needs its own bounded configuration.
  assert.deepEqual(viteConfig.optimizeDeps?.entries, ['index.html']);
  assert.equal(viteConfig.server?.watch?.followSymlinks, false);
  assert.ok(viteConfig.server?.watch?.ignored?.includes('**/target/**'));
  assert.match(styles, /^@import ['"]tailwindcss['"] source\(['"]\.\/['"]\);$/m);
  assert.match(tauriIgnore, /^\/target\/$/m);
  assert.match(tauriIgnore, /^\/\.flatpak-builder\/$/m);
});

test('Tauri and Vite use the same fixed development endpoint', () => {
  const tauriConfig = JSON.parse(read('src-tauri/tauri.conf.json'));
  const devUrl = new URL(tauriConfig.build.devUrl);

  assert.equal(devUrl.hostname, viteConfig.server?.host);
  assert.equal(Number(devUrl.port), viteConfig.server?.port);
  assert.equal(viteConfig.server?.strictPort, true);
});

test('the native startup background matches the dark application canvas', () => {
  const tauriConfig = JSON.parse(read('src-tauri/tauri.conf.json'));
  const index = read('index.html');
  const styles = read('frontend/styles.css');
  const background = tauriConfig.app.windows[0].backgroundColor;

  assert.equal(background, '#060b10');
  assert.match(index, new RegExp(`<meta name="theme-color" content="${background}"`));
  assert.match(styles, new RegExp(`--color-surface-canvas: ${background};`));
});

test('hosted workflows do not run speculative release cache builds', () => {
  const workflow = read('.github/workflows/build-desktop-artifacts.yml');
  assert.equal(
    existsSync(new URL('../.github/workflows/warm-release-caches.yml', import.meta.url)),
    false,
  );
  assert.doesNotMatch(workflow, /Swatinem\/rust-cache|shared-key:|save-if:/);
});

test('artifact workflow builds every supported release package', () => {
  const workflow = read('.github/workflows/build-desktop-artifacts.yml');
  const qualityWorkflow = read('.github/workflows/quality.yml');
  const qualityScript = read('scripts/quality.sh');
  const coverageScript = read('scripts/coverage.sh');
  const rustToolchain = read('rust-toolchain.toml');
  assert.match(rustToolchain, /^channel = "1\.98\.0"$/m);
  assert.match(rustToolchain, /^components = \["clippy", "llvm-tools-preview", "rustfmt"\]$/m);
  assert.match(qualityWorkflow, /name: Verify pinned Rust toolchain/);
  assert.match(workflow, /name: Verify pinned Rust toolchain/);
  assert.doesNotMatch(qualityWorkflow, /rustup update stable/);
  assert.doesNotMatch(workflow, /rustup update stable/);
  // The release no longer re-runs the portable gate. Both tag pushes and manual
  // tag dispatches verify the cheap tag contract and confirm that `Portable
  // quality` actually succeeded for this commit before any expensive package
  // build starts.
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /npm run version:check -- --tag/);
  assert.match(workflow, /RELEASE_TAG: \$\{\{ github\.ref_name \}\}/);
  assert.match(workflow, /git merge-base --is-ancestor "\$GITHUB_SHA"/);
  assert.match(workflow, /workflows\/quality\.yml\/runs\?head_sha=\$GITHUB_SHA/);
  assert.match(workflow, /actions\/runs\/\$run_id\/jobs\?per_page=100/);
  assert.match(workflow, /\.name == "Portable checks"/);
  assert.match(workflow, /\.name == "Rust checks"/);
  assert.match(workflow, /\.conclusion == "success" or \.conclusion == "skipped"/);
  assert.doesNotMatch(workflow, /require-quality:/);
  assert.match(workflow, /draft-release:[\s\S]*?needs: \[linux-appimage, windows-portable\]/);
  assert.doesNotMatch(workflow, /uses: \.\/\.github\/workflows\/quality\.yml/);
  // CI runs the two halves as parallel jobs; the Rust half is conditional.
  assert.match(qualityWorkflow, /run: \.\/scripts\/quality\.sh ci-portable/);
  assert.match(qualityWorkflow, /run: \.\/scripts\/quality\.sh ci-rust/);
  assert.match(qualityWorkflow, /if: needs\.changes\.outputs\.rust == 'true'/);
  assert.doesNotMatch(qualityWorkflow, /cache-apt-pkgs-action/);
  assert.match(qualityWorkflow, /sudo apt-get update/);
  assert.match(qualityWorkflow, /sudo apt-get install --yes/);
  // Cargo paths must trigger the hosted Rust job, and release publication checks
  // that the conditional job actually succeeded instead of trusting only the
  // workflow's overall conclusion.
  assert.match(qualityWorkflow, /Cargo\\\.\(toml\|lock\)/);
  assert.match(qualityWorkflow, /\\\.rs\$/);
  assert.doesNotMatch(qualityWorkflow, /run: \.\/scripts\/quality\.sh full/);
  assert.doesNotMatch(qualityWorkflow, /run: \.\/scripts\/coverage\.sh/);
  assert.doesNotMatch(qualityWorkflow, /npm ci/);
  assert.match(qualityScript, /npm ci --no-audit --no-fund/);
  assert.match(qualityScript, /\.\/scripts\/coverage\.sh/);
  assert.doesNotMatch(qualityWorkflow, /cargo-llvm-cov/);
  assert.doesNotMatch(qualityWorkflow, /cargo install cargo-llvm-cov/);
  assert.match(coverageScript, /cargo llvm-cov --locked --workspace --all-targets --all-features/);
  assert.match(coverageScript, /--no-clean --quiet --json/);
  assert.doesNotMatch(coverageScript, /--package/);
  assert.match(qualityWorkflow, /actions\/cache@[0-9a-f]{40}/);
  assert.match(qualityWorkflow, /Swatinem\/rust-cache@[0-9a-f]{40}/);
  assert.match(
    qualityWorkflow,
    /prefix-key: v1-rust\n\s+shared-key: portable-quality[\s\S]{0,240}?cache-workspace-crates: true/,
  );
  assert.equal([...workflow.matchAll(/actions\/cache@[0-9a-f]{40}/g)].length, 0);
  assert.equal([...workflow.matchAll(/Swatinem\/rust-cache@[0-9a-f]{40}/g)].length, 0);
  assert.match(
    workflow,
    /linux-appimage:[\s\S]*?needs: verify-release[\s\S]*?runs-on: ubuntu-22\.04/,
  );
  assert.match(workflow, /npm run build:appimage/);
  assert.doesNotMatch(workflow, /flatpak|\.deb|\.rpm/);
  assert.match(workflow, /windows-portable:[\s\S]*?needs: verify-release/);
  assert.doesNotMatch(workflow, /\n  macos:|runs-on: macos-|aarch64-apple-darwin/);
  assert.doesNotMatch(workflow, /linux-native:|npm run build:linux/);
  assert.match(workflow, /npm run build:windows/);
  assert.match(workflow, /artifacts\/WiCReplayViewer-linux-x86_64\.AppImage/);
  assert.doesNotMatch(workflow, /WiCReplayViewer-linux-.*\.(?:deb|rpm)|macos-(?:x64|arm64)/);
  assert.match(workflow, /WiCReplayViewer-windows-x64-portable\.zip/);
  assert.equal([...workflow.matchAll(/actions\/download-artifact@[0-9a-f]{40}/g)].length, 1);
  assert.match(workflow, /pattern: wic-replay-viewer-\*/);
  assert.match(workflow, /merge-multiple: true/);
  assert.match(workflow, /target\/release\/wic-replay-viewer\.exe/);
  assert.match(workflow, /gh release create/);
  assert.match(workflow, /--draft/);
  assert.match(workflow, /npm run --silent release:notes/);
  assert.match(workflow, /Refusing to replace assets on published release/);
  assert.match(workflow, /git fetch --no-tags --force origin "refs\/tags\/\$RELEASE_TAG"/);
  assert.match(workflow, /git rev-parse --verify 'FETCH_HEAD\^\{commit\}'/);
  assert.match(workflow, /"\$tag_commit" != "\$GITHUB_SHA"/);
  assert.match(workflow, /gh release delete-asset/);
  assert.match(workflow, /Draft release assets do not match the current build/);
  assert.doesNotMatch(workflow, /bundle\/nsis|bundle\/msi|windows-x64-installers/);

  const hostedWorkflows = `${workflow}\n${qualityWorkflow}`;
  assert.match(
    hostedWorkflows,
    /actions\/setup-node@249970729cb0ef3589644e2896645e5dc5ba9c38 # v6/,
  );
  assert.match(hostedWorkflows, /actions\/cache@caa296126883cff596d87d8935842f9db880ef25 # v5/);
  assert.match(
    hostedWorkflows,
    /actions\/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7\.0\.1/,
  );
  assert.match(
    hostedWorkflows,
    /actions\/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8\.0\.1/,
  );
  assert.doesNotMatch(
    hostedWorkflows,
    /actions\/(?:setup-node@[0-9a-f]{40} # v4|cache@[0-9a-f]{40} # v4|upload-artifact@[0-9a-f]{40} # v[456]|download-artifact@[0-9a-f]{40} # v[567])(?:\n|$)/,
  );

  const actionReferences = [workflow, qualityWorkflow].flatMap((contents) =>
    [...contents.matchAll(/^\s*- uses: ([^\s#]+)/gm)].map(([, reference]) => reference),
  );
  assert.ok(actionReferences.length > 0);
  for (const reference of actionReferences) {
    assert.match(reference, /^[^@]+@[0-9a-f]{40}$/);
  }

  const lines = workflow.split('\n');
  for (let index = 0; index < lines.length; index += 1) {
    const match = /^(\s*)run:\s*(.*)$/.exec(lines[index]);
    if (!match) {
      continue;
    }
    const indentation = match[1].length;
    const block = [match[2]];
    while (index + 1 < lines.length) {
      const next = lines[index + 1];
      if (next.trim() && next.length - next.trimStart().length <= indentation) {
        break;
      }
      block.push(next);
      index += 1;
    }
    assert.doesNotMatch(block.join('\n'), /\$\{\{/);
  }
});

test('private research data is excluded from application discovery and bundled resources', () => {
  const config = JSON.parse(read('src-tauri/tauri.conf.json'));
  assert.match(read('.gitignore'), /^\/local\/$/m);
  assert.match(read('.prettierignore'), /^local\/$/m);
  assert.match(read('.taurignore'), /^\/local\/$/m);
  assert.ok(viteConfig.server.watch.ignored.includes('**/local/**'));
  assert.deepEqual(Object.keys(config.bundle.resources).sort(), [
    '../LICENSE',
    '../NOTICE.txt',
    '../THIRD_PARTY_NOTICES.txt',
  ]);
});
