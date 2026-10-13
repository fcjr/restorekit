import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Server build of src/entry-server.ts for scripts/prerender.js. Asset imports
// resolve to the same hashed /assets/ urls as the client build.
export default defineConfig({
  plugins: [svelte()],
  build: {
    ssr: "src/entry-server.ts",
    outDir: "dist/ssr",
    emptyOutDir: true,
  },
});
