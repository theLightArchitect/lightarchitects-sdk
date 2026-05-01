import { mount } from 'svelte';
import App from './app.svelte';
import './styles/index.css';
import { resolveToken, initCookieSession } from '$lib/auth';

// Resolve Bearer token from URL hash on first load, then attempt to upgrade
// to an HttpOnly session cookie (v0.4.0 cookie rotation).  Exchange failure
// leaves the existing Bearer flow intact — no disruption to the user.
const token = resolveToken();
if (token) {
  initCookieSession(token).catch(() => {
    // Exchange failed — bearer mode stays active
  });
}

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
