// Entry point: global styles, then the app. The GUI uses the system font,
// so nothing else needs loading.
import './app.css';

import { mount } from 'svelte';
import App from './App.svelte';

mount(App, { target: document.getElementById('app')! });
