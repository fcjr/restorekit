import { render } from "svelte/server";
import App from "./App.svelte";
import DfuPort from "./pages/DfuPort.svelte";

// Rendered to html at build time by scripts/prerender.js. The home page is
// hydrated by main.ts; the pages are static.
export const home = () => render(App);

export const pages: Record<string, () => ReturnType<typeof render>> = {
  "dfu-port": () => render(DfuPort),
};
