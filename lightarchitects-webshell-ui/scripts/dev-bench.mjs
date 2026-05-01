#!/usr/bin/env node
// dev:bench — spawn the webshell dev server and the vitest UI dashboard
// side-by-side. Ctrl+C tears down both cleanly.
//
//   pnpm dev:bench
//
// Then point a browser tab at:
//   http://localhost:5173            ← real webshell (vite HMR)
//   http://localhost:51204/__vitest__/  ← vitest UI dashboard
//
// Both reload on file changes. Open them side-by-side (split-screen, two
// monitors, or any tiling tool) to see the test version of a component
// next to the real-app version.

import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const procs = [];

function launch(label, color, args) {
  const p = spawn('pnpm', ['exec', ...args], {
    cwd: ROOT,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: process.env,
  });
  const tag = `\x1b[${color}m[${label}]\x1b[0m`;
  p.stdout.on('data', (b) => process.stdout.write(`${tag} ${b}`));
  p.stderr.on('data', (b) => process.stderr.write(`${tag} ${b}`));
  p.on('exit', (code) => {
    if (code !== 0 && code !== null) {
      console.error(`${tag} exited with code ${code}`);
    }
    shutdown();
  });
  return p;
}

let shuttingDown = false;
function shutdown() {
  if (shuttingDown) return;
  shuttingDown = true;
  for (const p of procs) {
    if (!p.killed) p.kill('SIGTERM');
  }
  setTimeout(() => process.exit(0), 200);
}

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

console.log('\x1b[36m▸\x1b[0m dev:bench starting:');
console.log('  \x1b[32m[vite]\x1b[0m       webshell dev server  → http://localhost:5173');
console.log('  \x1b[35m[vitest-ui]\x1b[0m  test dashboard       → http://localhost:51204/__vitest__/');
console.log('  Ctrl+C tears down both.');
console.log('');

procs.push(launch('vite',      '32', ['vite']));
procs.push(launch('vitest-ui', '35', ['vitest', '--ui', '--no-open']));
