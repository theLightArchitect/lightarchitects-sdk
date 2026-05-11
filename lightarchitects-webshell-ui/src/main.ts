import { mount } from 'svelte';
import App from './app.svelte';
import './styles/index.css';
import { resolveToken, initCookieSession, initNonceSession } from '$lib/auth';

// Check for one-time nonce first (v0.5.0 — prevents raw token in MCP logs).
// Falls back to the legacy #token= path for backward compatibility.
const hash = window.location.hash.slice(1);
const hashParams = new URLSearchParams(hash);
const nonce = hashParams.get('nonce');
if (nonce) {
  history.replaceState(null, '', window.location.pathname + window.location.search);
  initNonceSession(nonce).catch(() => {});
} else {
  // Legacy path: resolve Bearer token from URL hash or sessionStorage.
  const token = resolveToken();
  if (token) {
    initCookieSession(token).catch(() => {
      // Exchange failed — bearer mode stays active
    });
  }
}

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
