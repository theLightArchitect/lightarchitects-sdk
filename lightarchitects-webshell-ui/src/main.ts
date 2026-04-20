import { mount } from 'svelte';
import App from './app.svelte';
import './styles/index.css';
import { resolveToken } from '$lib/auth';

// Resolve Bearer token from URL hash on first load.
resolveToken();

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;