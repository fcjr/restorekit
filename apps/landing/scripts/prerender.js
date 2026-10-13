// Bakes the rendered html into the client build so crawlers that don't run JS
// still see the page: the home page goes into index.html's #app (main.ts
// hydrates it), and each page in entry-server's `pages` becomes <name>.html,
// which Cloudflare serves at /<name>.
import { readFile, writeFile } from "node:fs/promises";

const client = new URL("../dist/client/", import.meta.url);
const { home, pages } = await import("../dist/ssr/entry-server.js");

const shell = await readFile(new URL("index.html", client), "utf8");
const mount = '<div id="app"></div>';
if (!shell.includes(mount)) throw new Error(`no ${mount} in dist/client/index.html`);
await writeFile(new URL("index.html", client), shell.replace(mount, `<div id="app">${home().body}</div>`));

const styles = shell.match(/<link rel="stylesheet"[^>]*>/g) ?? [];
for (const [name, page] of Object.entries(pages)) {
  const { head, body } = page();
  const html = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
    <meta name="theme-color" content="#103d2e" />
    ${styles.join("\n    ")}
    ${head}
  </head>
  <body>
    ${body}
  </body>
</html>
`;
  await writeFile(new URL(`${name}.html`, client), html);
}
