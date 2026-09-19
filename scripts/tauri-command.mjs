import { fileURLToPath } from 'node:url';
import process from 'node:process';

const defaultCliPath = fileURLToPath(
  new URL('../node_modules/@tauri-apps/cli/tauri.js', import.meta.url),
);

export function tauriInvocation(args, nodePath = process.execPath, cliPath = defaultCliPath) {
  return {
    args: [cliPath, ...args],
    command: nodePath,
  };
}
