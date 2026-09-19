import assert from 'node:assert/strict';
import test from 'node:test';

import { tauriInvocation } from './tauri-command.mjs';
import { viewerEnvironment } from './tauri-environment.mjs';

test('Tauri uses the JavaScript CLI instead of a platform command shim', () => {
  const invocation = tauriInvocation(
    ['build', '--ci', '--no-bundle'],
    'C:\\Program Files\\nodejs\\node.exe',
    'D:\\repo\\node_modules\\@tauri-apps\\cli\\tauri.js',
  );

  assert.equal(invocation.command, 'C:\\Program Files\\nodejs\\node.exe');
  assert.deepEqual(invocation.args, [
    'D:\\repo\\node_modules\\@tauri-apps\\cli\\tauri.js',
    'build',
    '--ci',
    '--no-bundle',
  ]);
});

test('Linux viewer defaults to Wayland before spawning Tauri', () => {
  const environment = viewerEnvironment('linux', { GDK_BACKEND: 'x11' });

  assert.equal(environment.GDK_BACKEND, 'wayland');
  assert.equal(environment.WEBKIT_DMABUF_RENDERER_FORCE_SHM, '1');
});

test('viewer-specific backend and WebKit overrides remain authoritative', () => {
  const environment = viewerEnvironment('linux', {
    GDK_BACKEND: 'wayland',
    WIC_REPLAY_VIEWER_GDK_BACKEND: 'x11',
    WEBKIT_DMABUF_RENDERER_FORCE_SHM: '0',
  });

  assert.equal(environment.GDK_BACKEND, 'x11');
  assert.equal(environment.WEBKIT_DMABUF_RENDERER_FORCE_SHM, '0');
});

test('non-Linux environments are unchanged', () => {
  const source = { GDK_BACKEND: 'custom', VALUE: 'kept' };

  assert.deepEqual(viewerEnvironment('darwin', source), source);
});
