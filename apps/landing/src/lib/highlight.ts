import type { HighlighterCore } from "shiki/core";

// Brand-token theme so highlighted code reads like the rest of the page.
const theme = {
  name: "restorekit",
  type: "dark" as const,
  colors: {
    "editor.background": "#00000000",
    "editor.foreground": "#d7d2c4",
  },
  settings: [
    { settings: { foreground: "#d7d2c4" } },
    { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: "#77766a" } },
    { scope: ["string", "punctuation.definition.string"], settings: { foreground: "#7ba86a" } },
    {
      scope: ["keyword", "storage.type", "storage.modifier", "keyword.operator"],
      settings: { foreground: "#e8a33d" },
    },
    { scope: ["constant.numeric", "constant.language"], settings: { foreground: "#6a93a8" } },
    {
      scope: ["entity.name.function", "support.function", "meta.function-call"],
      settings: { foreground: "#e6e2d6" },
    },
    { scope: ["variable", "variable.other"], settings: { foreground: "#d7d2c4" } },
    { scope: ["entity.name.type", "support.type"], settings: { foreground: "#6a93a8" } },
  ],
};

let highlighter: Promise<HighlighterCore> | null = null;

// shiki is loaded on demand so it stays out of the main bundle; blocks render
// as plain text until it arrives.
async function create(): Promise<HighlighterCore> {
  const [{ createHighlighterCore }, { createJavaScriptRegexEngine }, bash, rust] =
    await Promise.all([
      import("shiki/core"),
      import("shiki/engine/javascript"),
      import("shiki/langs/bash.mjs"),
      import("shiki/langs/rust.mjs"),
    ]);
  return createHighlighterCore({
    themes: [theme],
    langs: [bash.default, rust.default],
    engine: createJavaScriptRegexEngine(),
  });
}

export function highlight(code: string, lang: "bash" | "rust"): Promise<string> {
  highlighter ??= create();
  return highlighter.then((h) => h.codeToHtml(code, { lang, theme: "restorekit" }));
}
