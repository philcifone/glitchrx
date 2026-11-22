// Entry point: the heading font, global styles, then the app. Body text
// uses the system font, so Young Serif is the only font bundled.
import '@fontsource/young-serif/400.css';
import './app.css';

import { mount } from 'svelte';
import App from './App.svelte';

mount(App, { target: document.getElementById('app')! });

// Fade out the launch splash from index.html, then drop it. A frame first,
// so the app has painted underneath.
const splash = document.getElementById('splash');
if (splash) {
  requestAnimationFrame(() => {
    splash.classList.add('gone');
    setTimeout(() => splash.remove(), 400);
  });
}
