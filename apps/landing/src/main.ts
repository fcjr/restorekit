import { hydrate, mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

// The build prerenders the page into #app (scripts/prerender.js); the dev
// server hands it over empty.
const target = document.getElementById("app")!;
const app = target.firstChild ? hydrate(App, { target }) : mount(App, { target });

export default app;
