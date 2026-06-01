import { mount } from 'svelte';
import App from './app.svelte';
import './styles/index.css';
import { resolveToken, initCookieSession, initNonceSession } from '$lib/auth';

// Check for one-time nonce first (v0.5.0 — prevents raw token in MCP logs).
// Falls back to the legacy #token= path for backward compatibility.
const hash = window.location.hash.slice(1);
const hashParams = new URLSearchParams(hash);
const nonce = hashParams.get('nonce');

// Resolve auth before mounting so no component fires an unauthenticated API call
// on first render (race: nonce exchange is async, mount was previously synchronous).
let authReady: Promise<void>;
if (nonce) {
  // Strip nonce from URL before app mounts so it's never logged or bookmarked.
  history.replaceState(null, '', window.location.pathname + window.location.search);
  authReady = initNonceSession(nonce).catch(() => {});
} else {
  const token = resolveToken();
  authReady = token
    ? initCookieSession(token).catch(() => {
        // Exchange failed — bearer mode stays active; app still mounts.
      })
    : Promise.resolve();
}

export default authReady.then(() =>
  mount(App, { target: document.getElementById('app')! })
);
