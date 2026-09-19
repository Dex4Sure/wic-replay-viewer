#!/usr/bin/env node

import { spawn } from 'node:child_process';
import process from 'node:process';

import { tauriInvocation } from './tauri-command.mjs';
import { viewerEnvironment } from './tauri-environment.mjs';

const environment = viewerEnvironment(process.platform, process.env);
const invocation = tauriInvocation(process.argv.slice(2));

const child = spawn(invocation.command, invocation.args, {
  env: environment,
  stdio: 'inherit',
});

child.on('error', (error) => {
  console.error(`Failed to start the Tauri CLI: ${error.message}`);
  process.exitCode = 1;
});

child.on('exit', (code, signal) => {
  if (signal !== null) {
    process.kill(process.pid, signal);
    return;
  }
  process.exitCode = code ?? 1;
});
