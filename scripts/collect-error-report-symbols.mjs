import { copyFileSync, mkdirSync, readdirSync, renameSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('..', import.meta.url));
const source = path.join(root, 'dist/assets');
const destination = path.join(root, 'artifacts/diagnostic-symbols/frontend');
mkdirSync(destination, { recursive: true });
for (const name of readdirSync(source)) {
  if (name.endsWith('.js.map')) {
    copyFileSync(path.join(source, name.slice(0, -4)), path.join(destination, name.slice(0, -4)));
    renameSync(path.join(source, name), path.join(destination, name));
  }
}
