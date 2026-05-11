// e2e/fixtures/server.ts — Spawn the webshell binary on an ephemeral port for
// true end-to-end Playwright tests against the real Rust backend.
//
// Uses Node’s net.createServer().listen(0) to reserve an available TCP port,
// then passes it explicitly to the binary via --port <port>.  This avoids the
// --port 0 port-reporting bug (the binary logs 0 instead of the ephemeral port).

import { createServer } from 'net';
import { spawn, type ChildProcess } from 'child_process';
import { readFileSync } from 'fs';
import { join } from 'path';
import { homedir } from 'os';

const TOKEN_PATH = join(homedir(), '.lightarchitects', 'webshell', '.token');
const WEBSHELL_BIN = join(homedir(), '.lightarchitects', 'bin', 'lightarchitects-webshell');

function getToken(): string {
  try {
    return readFileSync(TOKEN_PATH, 'utf-8').trim();
  } catch {
    return process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
  }
}

function getAvailablePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address();
      const port = typeof addr === 'object' && addr !== null ? addr.port : 0;
      server.close((err) => {
        if (err) reject(err);
        else resolve(port);
      });
    });
    server.on('error', reject);
  });
}

export interface ServerPool {
  baseUrl: string;
  token: string;
  teardown: () => Promise<void>;
}

let globalChild: ChildProcess | null = null;

function ensureCleanup(child: ChildProcess) {
  globalChild = child;
  const doKill = () => {
    try {
      if (!child.killed) child.kill('SIGKILL');
    } catch {
      // ignore
    }
  };
  process.once('exit', doKill);
  process.once('SIGINT', doKill);
  process.once('SIGTERM', doKill);
}

export async function startServerPool(): Promise<ServerPool> {
  const port = await getAvailablePort();
  const token = getToken();

  const child = spawn(WEBSHELL_BIN, ['--port', String(port)], {
    detached: false,
    env: { ...process.env, LIGHTARCHITECTS_WEBSHELL_TOKEN: token },
  });

  ensureCleanup(child);

  // Wait for the server to log that it is listening (stderr, via tracing).
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      try { child.kill('SIGKILL'); } catch { /* ignore */ }
      reject(new Error(`Server failed to start on port ${port} within 15s`));
    }, 15000);

    const onData = (data: Buffer) => {
      const text = data.toString();
      if (text.includes('webshell server listening')) {
        clearTimeout(timeout);
        child.stderr?.off('data', onData);
        child.stdout?.off('data', onData);
        resolve();
      }
    };

    child.stderr?.on('data', onData);
    child.stdout?.on('data', onData);
    child.on('error', (err) => {
      clearTimeout(timeout);
      reject(err);
    });
    child.on('exit', (code) => {
      if (code !== null && code !== 0) {
        clearTimeout(timeout);
        reject(new Error(`Server exited with code ${code}`));
      }
    });
  });

  // Give Axum a few more ms to finish routing setup.
  await new Promise((r) => setTimeout(r, 300));

  const teardown = async () => {
    try {
      if (!child.killed) child.kill('SIGKILL');
    } catch {
      // ignore
    }
    await new Promise<void>((r) => child.on('exit', () => r()));
    globalChild = null;
  };

  return {
    baseUrl: `http://localhost:${port}`,
    token,
    teardown,
  };
}
