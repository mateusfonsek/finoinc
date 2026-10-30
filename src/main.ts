import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { platform } from "$lib/platform";

// Per-OS CSS (title bar inset) keys off this class, set before the first paint.
document.documentElement.classList.add(`platform-${platform}`);

const app = mount(App, { target: document.getElementById("app")! });

export default app;
