<script lang="ts">
  import type { Snippet } from "svelte";
  import { GITHUB, RELEASES, SPONSOR } from "../lib/links";

  // Header and footer for every page. The home page is hydrated (main.ts);
  // the pages under src/pages are rendered to static html at build time
  // (scripts/prerender.js) and ship no JS.
  let { children }: { children: Snippet } = $props();
</script>

<header class="sticky top-0 z-50 border-b border-copper bg-mask/95 backdrop-blur">
  <nav class="mx-auto flex h-16 max-w-6xl items-center gap-6 px-5">
    <a href="/" class="flex items-center gap-2.5 text-silk">
      <svg viewBox="0 0 32 32" width="20" height="20" aria-hidden="true">
        <rect x="7" y="7" width="18" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="1.8" />
        <path d="M4 16 H10 L12.2 11 L16 21 L19 16 H28" fill="none" stroke="var(--color-gold)" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span class="wide text-[17px] font-bold">restorekit</span>
    </a>
    <div class="grow"></div>
    <div class="hidden items-center gap-6 text-[15px] text-silk2 md:flex">
      <a href="/#devices" class="hover:text-silk">Devices</a>
      <a href="/#dongle" class="hover:text-silk">Dongle</a>
      <a href="/#how" class="hover:text-silk">How it works</a>
      <a href="/#desktop" class="hover:text-silk">App</a>
      <a href="/#cli" class="hover:text-silk">CLI</a>
      <a href="/#install" class="hover:text-silk">Install</a>
    </div>
    <a href={GITHUB} class="text-[15px] font-semibold text-gold hover:text-gold-hi">GitHub</a>
  </nav>
</header>

<main>
  {@render children()}
</main>

<footer class="border-t border-copper bg-mask-deep">
  <div class="mx-auto flex max-w-6xl flex-col gap-4 px-5 py-8 text-[14.5px] md:flex-row md:items-center">
    <a href="/" class="wide font-bold text-silk">restorekit</a>
    <div class="grow"></div>
    <div class="flex flex-wrap gap-x-6 gap-y-2 text-silk2">
      <a href="/#dongle" class="hover:text-silk">Reserve dongle-pro</a>
      <a href="/dfu-port" class="hover:text-silk">Find the DFU port</a>
      <a href={GITHUB} class="hover:text-silk">GitHub</a>
      <a href={SPONSOR} class="hover:text-silk">Sponsor</a>
      <a href={RELEASES} class="hover:text-silk">Releases</a>
      <a href="https://crates.io/crates/restorekit-cli" class="hover:text-silk">crates.io</a>
      <a href="https://docs.rs/restorekit" class="hover:text-silk">docs.rs</a>
    </div>
  </div>
  <p class="mx-auto max-w-6xl px-5 pb-8 t-small">
    The DFU code is a rust port of Asahi Linux's macvdmtool (also Apache-2.0), thank you to that
    team. Mac, iPhone, iPad and Apple Silicon are trademarks of Apple Inc., Acroname and Cambrionix
    of their respective owners, mentioned here for comparison only.
  </p>
</footer>
