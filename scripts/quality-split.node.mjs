import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('./quality.sh', import.meta.url));

const plan = (mode) => {
  const output = execFileSync(script, [mode], {
    encoding: 'utf8',
    env: { ...process.env, WIC_QUALITY_PRINT_PLAN: '1' },
  }).trim();
  return Object.fromEntries(
    output.split(' ').map((pair) => {
      const [name, value] = pair.split('=');
      return [name, value === 'true'];
    }),
  );
};

// CI keeps every full-gate stage except the deliberately local-only LLVM
// coverage measurement. If another stage were dropped from both halves, or
// added to one without being part of `full`, CI would silently drift.
test('the CI halves cover the full gate except local-only Rust coverage', () => {
  const full = plan('full');
  const portable = plan('ci-portable');
  const rust = plan('ci-rust');

  for (const stage of Object.keys(full)) {
    if (stage === 'rust_coverage') {
      assert.equal(full[stage], true);
      assert.equal(portable[stage] || rust[stage], false);
      continue;
    }
    assert.equal(
      portable[stage] || rust[stage],
      full[stage],
      `stage ${stage}: ci-portable/ci-rust must together match full`,
    );
  }
});

test('the Rust half owns hosted Rust tests and Clippy, but not LLVM coverage', () => {
  assert.equal(plan('ci-rust').rust, true);
  assert.equal(plan('ci-portable').rust, false);
  assert.equal(plan('ci-rust').rust_coverage, false);
  assert.equal(plan('ci-portable').rust_coverage, false);
  // The expensive half must be genuinely skippable: the portable half may not
  // depend on the WebKit toolchain that only the Rust half installs.
  assert.equal(plan('ci-portable').static, true);
  assert.equal(plan('ci-rust').static, false);
  assert.equal(plan('ci-portable').fetch, true);
  assert.equal(plan('ci-rust').fetch, true);
});

test('the local hook gates are unchanged', () => {
  const fast = plan('fast');
  assert.deepEqual(fast, {
    static: true,
    portable: false,
    rust: false,
    rust_coverage: false,
    fetch: false,
    node_modules: false,
    frontend: false,
  });
  const full = plan('full');
  assert.equal(full.static && full.portable && full.rust, true);
  assert.equal(full.rust_coverage, true);
});
