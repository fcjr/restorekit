<script lang="ts">
  import { onMount } from "svelte";
  import Code from "./components/Code.svelte";
  import shotRestoreDark from "./assets/app-restore-dark.png";
  import shotDevicesDark from "./assets/app-devices-dark.png";
  import shotHistoryDark from "./assets/app-history-dark.png";
  import shotRestoreLight from "./assets/app-restore-light.png";
  import leftshiftLogo from "./assets/leftshift.svg";
  import sceneDongle from "./assets/scene-dongle.webp";
  import sceneRecovery from "./assets/scene-recovery.webp";

  const GITHUB = "https://github.com/fcjr/restorekit";
  const RELEASES = `${GITHUB}/releases`;
  const SPONSOR = "https://github.com/sponsors/fcjr";

  // Direct links to the desktop-app builds, resolved from the GitHub API on
  // load. Each link points at the newest release that actually has that build
  // (a fresh release may still be missing assets while CI runs); until the
  // fetch resolves (or if it fails) they land on the releases page.
  let appVersion = $state("");
  let appDownloads = $state(
    [
      { label: "macOS (.dmg)", pattern: /_aarch64\.dmg$/ },
      { label: "Windows (setup.exe)", pattern: /_x64-setup\.exe$/ },
      { label: "Linux (.deb)", pattern: /_amd64\.deb$/ },
      { label: "Linux (.AppImage)", pattern: /_amd64\.AppImage$/ },
    ].map((d) => ({ ...d, url: RELEASES })),
  );

  // The hero offers the download and install command for whatever OS the
  // visitor is on, with the others one click away.
  type Host = "mac" | "windows" | "linux";
  const hosts: { id: Host; label: string; lines: string; dl: number }[] = [
    { id: "mac", label: "macOS", lines: "brew trust fcjr/fcjr\nbrew install fcjr/fcjr/restorekit-cli", dl: 0 },
    {
      id: "windows",
      label: "Windows",
      lines: "scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr\nscoop install restorekit-cli",
      dl: 1,
    },
    { id: "linux", label: "Linux", lines: "sudo snap install restorekit-cli", dl: 2 },
  ];
  let host = $state<Host>("mac");
  const heroHost = $derived(hosts.find((h) => h.id === host) ?? hosts[0]);

  onMount(async () => {
    webgl = hasWebgl();
    const ua = navigator.userAgent;
    if (/Windows/.test(ua)) host = "windows";
    else if (/Linux/.test(ua) && !/Android/.test(ua)) host = "linux";
    try {
      const res = await fetch("https://api.github.com/repos/fcjr/restorekit/releases?per_page=10");
      if (!res.ok) return;
      const rels: {
        tag_name?: string;
        draft?: boolean;
        prerelease?: boolean;
        assets?: { name: string; browser_download_url: string }[];
      }[] = await res.json();
      const stable = rels.filter((r) => !r.draft && !r.prerelease);
      appVersion = stable[0]?.tag_name ?? "";
      appDownloads = appDownloads.map((d) => {
        for (const rel of stable) {
          const asset = rel.assets?.find((a) => d.pattern.test(a.name));
          if (asset) return { ...d, url: asset.browser_download_url };
        }
        return d;
      });
    } catch {
      /* keep releases-page links */
    }
  });

  let copied = $state("");
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  function copy(id: string, text: string) {
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        copied = id;
        clearTimeout(copyTimer);
        copyTimer = setTimeout(() => (copied = ""), 1400);
      })
      .catch(() => {});
  }

  // dongle-lite vs the DFU-capable USB hubs people usually end up pricing out.
  const hubCols = ["dongle-lite", "Acroname USBHub 3c", "Cambrionix ThunderSync5 C16 PD"];
  const hubRows: [string, string, string, string][] = [
    ["Targets at once", "One", "6 ports, kit rated for 5", "16 ports"],
    ["Host machine", "Any linux, windows or mac", "Windows, mac or linux", "Windows, mac or linux"],
    [
      "Software to trigger DFU",
      "Nothing extra",
      "PD-logging license, included in the restore kit",
      "Connect Premium, 3 years included with the hub",
    ],
    [
      "Hardware cost",
      "Batch 2 not priced yet",
      "$1,200 hub, $1,999 restore kit",
      "£1,999, or £79/mo with a deposit",
    ],
    ["Open hardware", "Yes, files in the repo", "No", "No"],
    ["Serial console", "Target UART over SBU", "—", "—"],
    ["Built for", "One mac, at a bench or in a bag", "A bench of macs", "A rack of macs"],
  ];


  // The 3D scenes are decoration, so three.js only loads when a section is
  // close to the viewport and the browser actually has WebGL.
  function hasWebgl(): boolean {
    try {
      const probe = document.createElement("canvas");
      return !!(probe.getContext("webgl2") ?? probe.getContext("webgl"));
    } catch {
      return false;
    }
  }
  function lazyScene(host: () => HTMLElement | undefined, show: () => void) {
    $effect(() => {
      const el = host();
      if (!el || !hasWebgl()) return;
      const io = new IntersectionObserver(
        (entries) => {
          if (entries.some((e) => e.isIntersecting)) {
            show();
            io.disconnect();
          }
        },
        { rootMargin: "300px" },
      );
      io.observe(el);
      return () => io.disconnect();
    });
  }
  // Without WebGL (hardware acceleration off, blocklisted GPU) the scenes
  // show a still frame instead of an empty box.
  let webgl = $state(true);
  let sceneHost = $state<HTMLElement>();
  let showScene = $state(false);
  lazyScene(
    () => sceneHost,
    () => (showScene = true),
  );
  let dongleHost = $state<HTMLElement>();
  let showDongle = $state(false);
  lazyScene(
    () => dongleHost,
    () => (showDongle = true),
  );

  // dongle-lite preorder-interest form → POST /api/reserve → D1.
  let reserveEmail = $state("");
  let reserveState = $state<"idle" | "busy" | "done" | "error">("idle");
  let reserveError = $state("");
  async function reserve(e: SubmitEvent) {
    e.preventDefault();
    if (reserveState === "busy" || reserveState === "done") return;
    reserveState = "busy";
    try {
      const res = await fetch("/api/reserve", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email: reserveEmail }),
      });
      if (!res.ok) {
        const body: { error?: string } = await res.json().catch(() => ({}));
        throw new Error(body.error ?? "");
      }
      reserveState = "done";
    } catch (err) {
      reserveState = "error";
      reserveError =
        err instanceof Error && err.message ? err.message : "something broke, mind trying again?";
    }
  }

  const dfuPorts = [
    ["14″ / 16″ MacBook Pro", "Left side, port next to MagSafe"],
    ["Mac mini / Studio", "Port closest to the power button"],
    ["MacBook Air / 13″ Pro", "Left side, port closest to the hinge"],
    ["iMac", "Port closest to the edge"],
  ];
</script>

{#snippet cmd(id: string, lines: string, display: string)}
  <div class="group relative rounded-md border border-copper bg-mask-deep text-left">
    <Code code={display} lang="bash" />
    <button
      class="absolute top-2 right-2 rounded-[4px] border border-copper-hi bg-mask px-2.5 py-1 text-[12px] text-silk3 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:border-gold hover:text-gold"
      onclick={() => copy(id, lines)}
    >
      {copied === id ? "Copied" : "Copy"}
    </button>
  </div>
{/snippet}

{#snippet still(src: string)}
  <img {src} alt="" class="absolute inset-0 h-full w-full object-cover" loading="lazy" />
{/snippet}

{#snippet pad()}
  <span class="mt-[0.55em] inline-block h-[7px] w-[11px] shrink-0 rounded-[2px] bg-gold" aria-hidden="true"></span>
{/snippet}

<!-- nav -->
<header class="sticky top-0 z-50 border-b border-copper bg-mask/95 backdrop-blur">
  <nav class="mx-auto flex h-16 max-w-6xl items-center gap-6 px-5">
    <a href="#top" class="flex items-center gap-2.5 text-silk">
      <svg viewBox="0 0 32 32" width="20" height="20" aria-hidden="true">
        <rect x="7" y="7" width="18" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="1.8" />
        <path d="M4 16 H10 L12.2 11 L16 21 L19 16 H28" fill="none" stroke="var(--color-gold)" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span class="wide text-[17px] font-bold">restorekit</span>
    </a>
    <div class="grow"></div>
    <div class="hidden items-center gap-6 text-[15px] text-silk2 md:flex">
      <a href="#dongle" class="hover:text-silk">Dongle</a>
      <a href="#how" class="hover:text-silk">How it works</a>
      <a href="#desktop" class="hover:text-silk">App</a>
      <a href="#cli" class="hover:text-silk">CLI</a>
      <a href="#install" class="hover:text-silk">Install</a>
    </div>
    <a href={GITHUB} class="text-[15px] font-semibold text-gold hover:text-gold-hi">GitHub</a>
  </nav>
</header>

<main id="top">
  <!-- hero: copper showing through the mask behind the headline -->
  <section class="relative overflow-hidden">
    <svg
      class="pointer-events-none absolute inset-0 h-full w-full"
      viewBox="0 0 1200 700"
      preserveAspectRatio="xMidYMax slice"
      fill="none"
      aria-hidden="true"
    >
      <g stroke="var(--color-copper)" stroke-width="5" stroke-linecap="round" stroke-linejoin="round">
        {#each [0, 1, 2, 3] as i (i)}
          <path d="M-20 {560 + i * 16} H{540 + i * 6.6} l130 -130 H1220" />
        {/each}
        {#each [0, 1, 2] as i (i)}
          <path d="M1220 {50 + i * 16} H{1160 - i * 6.6} l-60 60 H{1080 + i * 6.6}" />
        {/each}
        <path d="M60 700 V660 l40 -40 H300 l50 -50" />
        <path d="M-20 150 H40 l50 50 V300" />
      </g>
      <g fill="var(--color-mask-deep)" stroke="var(--color-copper-hi)" stroke-width="4">
        {#each [0, 1, 2] as i (i)}
          <circle cx={1080 + i * 6.6} cy={110 + i * 16} r="6" />
        {/each}
        <circle cx="350" cy="570" r="7" />
        <circle cx="90" cy="300" r="7" />
      </g>
    </svg>

    <div
      class="relative mx-auto grid max-w-6xl items-center gap-12 px-5 pt-14 pb-16 md:pt-20 md:pb-24 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] lg:gap-14"
    >
      <div>
        <h1 class="t-display">Wipe and restore any mac from any computer.</h1>
        <p class="t-lead mt-6 max-w-[44ch]">
          Works on T2 and Apple Silicon macs from macOS, linux or windows, without any apple tools. It's free and open source.
        </p>

        <div class="mt-8 flex flex-col gap-3 sm:flex-row">
          <a href={appDownloads[heroHost.dl].url} class="btn-pad">Download for {heroHost.label}</a>
          <a href="#dongle" class="btn-line">Reserve a dongle</a>
        </div>

        <div class="mt-8 max-w-md">
          <div class="mb-2 flex items-center gap-1" role="group" aria-label="Install the cli on">
            <span class="mr-2 text-[14px] text-silk3">Or the cli on</span>
            {#each hosts as h (h.id)}
              <button
                class="rounded-[4px] px-2.5 py-1 text-[14px] transition-colors {host === h.id
                  ? 'bg-copper text-silk'
                  : 'text-silk3 hover:text-silk'}"
                aria-pressed={host === h.id}
                onclick={() => (host = h.id)}
              >
                {h.label}
              </button>
            {/each}
          </div>
          {@render cmd("hero", heroHost.lines, heroHost.lines.split("\n").map((l) => `$ ${l}`).join("\n"))}
        </div>
      </div>

      <img
        src={shotRestoreDark}
        alt="restorekit desktop app restoring a MacBook Pro, 42% through sending the filesystem, with two more macs queued"
        class="block w-full rounded-lg border border-silk/15 shadow-[0_30px_60px_-20px_rgba(4,20,14,0.8)] lg:w-[128%] lg:max-w-none"
        width="1720"
        height="1240"
        fetchpriority="high"
      />
    </div>
  </section>

  <!-- dongle-lite -->
  <section id="dongle" class="bg-mask-deep">
    <div class="mx-auto grid max-w-6xl items-start gap-10 px-5 py-20 lg:grid-cols-[1fr_1.3fr]">
      <div>
        <h2 class="t-h2">
          Automatic DFU from any computer. Yes, even windows.
        </h2>
        <p class="mt-5 max-w-[56ch]">
          Putting a mac into DFU takes a USB-PD message that normal PC ports can't send, so you've always needed a second mac. dongle-lite sends it for you. It also exposes apple's serial console, so you can see why a restore failed.
        </p>
        <p class="mt-4 max-w-[56ch]">
          Buying one from me is what keeps restorekit going, so I'd love it if you did. But it's
          open hardware, so if you'd rather fire up the soldering iron, the
          <a href="{GITHUB}/tree/main/hardware/dongle-lite" class="lnk">schematics and fab files</a>
          are all in the repo. I won't be offended. Much.
        </p>

        <dl class="mt-7 grid grid-cols-[auto_1fr] gap-x-6 gap-y-2.5 t-data">
          <dt class="text-silk3">Ports</dt>
          <dd class="text-silk">Two usb-c, one to your computer and one to the mac</dd>
          <dt class="text-silk3">Chip</dt>
          <dd class="text-silk">RP2350 running rust firmware</dd>
          
          <dt class="text-silk3">Size</dt>
          <dd class="text-silk">77 × 22 mm</dd>
        </dl>

        <div class="mt-9 rounded-lg border border-copper bg-mask p-5">
          <p class="t-data text-silk">
            Batch 1 is gone. Get one email when batch 2 is ready.
          </p>
          <div class="mt-4">
            {#if reserveState === "done"}
              <p class="text-ok">You're on the batch 2 list. Talk soon.</p>
            {:else}
              <form onsubmit={reserve} class="flex flex-col gap-2.5 sm:flex-row">
                <label class="sr-only" for="reserve-email">Email</label>
                <input
                  id="reserve-email"
                  type="email"
                  required
                  placeholder="you@example.com"
                  autocomplete="email"
                  bind:value={reserveEmail}
                  class="grow rounded-[5px] border border-copper-hi bg-mask-deep px-4 py-2.5 text-silk placeholder:text-silk3/70 focus:border-gold focus:outline-none"
                />
                <button
                  type="submit"
                  disabled={reserveState === "busy"}
                  class="btn-pad shrink-0 disabled:opacity-60"
                >
                  {reserveState === "busy" ? "Saving..." : "Join the batch 2 list"}
                </button>
              </form>
              {#if reserveState === "error"}
                <p class="t-small mt-2 !text-danger">{reserveError}</p>
              {/if}
            {/if}
          </div>
          
        </div>
      </div>

      <figure>
        <div
          bind:this={dongleHost}
          class="relative h-[300px] overflow-hidden rounded-lg border border-copper bg-mask-deep sm:h-[380px] lg:h-[480px]"
          aria-hidden="true"
        >
          {#if showDongle}
            {#await import("./components/DongleScene.svelte") then Mod}
              <Mod.default />
            {:catch}
              {@render still(sceneDongle)}
            {/await}
          {:else if !webgl}
            {@render still(sceneDongle)}
          {/if}
        </div>
        <figcaption class="t-small mt-3">
          The actual board, straight from the KiCad files.
        </figcaption>
      </figure>
    </div>
  </section>

  <!-- dongle vs hubs -->
  <section id="hubs" class="bg-mask">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2 max-w-3xl">
        Compared to DFU hubs
      </h2>
      <p class="mt-5 max-w-[62ch]">
        <a href="https://acroname.com/store/s106-usbhub-3c-kit" class="lnk">Acroname</a>
        and
        <a href="https://www.cambrionix.com/products/thundersync5-c16-pd" class="lnk">Cambrionix</a>
        make hubs that work at scale, for racks of macs. They've also had DFU hardware to
        themselves for years, and they price like it. Acroname charges $400 for the software
        license that switches DFU on. Cambrionix's DFU hub is £1,999. Triggering DFU is one USB-PD
        message. It shouldn't cost that much. dongle-lite does it from the computer you already
        have, with nothing to license.
      </p>

      <div class="mt-10 overflow-x-auto">
        <table class="w-full min-w-[720px] border-collapse text-left t-data">
          <thead>
            <tr>
              <th class="w-[22%] pb-3"></th>
              {#each hubCols as col, i (col)}
                <th class="pb-3 pr-4 align-bottom font-semibold {i === 0 ? 'wide text-gold' : 'text-silk'}">
                  {col}
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each hubRows as [label, ...cells] (label)}
              <tr class="border-t border-copper">
                <th scope="row" class="py-3.5 pr-4 font-normal text-silk3">{label}</th>
                {#each cells as cell, i (i)}
                  <td class="py-3.5 pr-4 {i === 0 ? 'text-silk' : 'text-silk2'}">{cell}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <p class="mt-6 max-w-[78ch] t-small">
        Prices from vendor sites, August 2026. Acroname sells the
        <a href="https://acroname.com/store/t99-pd-log" class="lnk">PD-logging license</a> on its
        own, and their
        <a href="https://acroname.com/store/s106-usbhub-3c-kit" class="lnk">Apple restore kit</a>
        includes it. Cambrionix doesn't publish what Connect Premium costs after the first three years.
      </p>
    </div>
  </section>

  <!-- why -->
  <section class="bg-mask-deep">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-20 md:grid-cols-[1fr_1.2fr]">
      <h2 class="t-h2">
        People should be able to repair their mac without needing to own another one!
      </h2>
      <div class="t-lead max-w-[58ch] space-y-5">
        <p>
          I've worked at a few places where windows was the default machine (including for IT) and
          macs were only issued when requested or required. A lot of times the IT folks are stuck
          carrying a macbook for one job, recovering and resetting the rest of the fleet.
        </p>
        <p>
          I've also seen companies ship a whole new mac to an employee with no apple store nearby
          when a simple reset would have fixed it.
        </p>
        <p class="t-h3">I think this sucks.</p>
        <p>
          So the software is free and Apache-2.0, and it'll stay that way.
          <a href="#dongle" class="lnk">dongle-lite</a> is the part you can buy, and selling it pays for my time on the rest.
        </p>
        
      </div>
    </div>
  </section>

  <!-- how it works -->
  <section id="how" class="bg-mask">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2">
        Plug it in, follow the instructions, and bam!
      </h2>

      <div
        bind:this={sceneHost}
        class="relative mt-10 h-[280px] overflow-hidden rounded-lg border border-copper bg-mask-deep md:h-[380px]"
        aria-hidden="true"
      >
        {#if showScene}
          {#await import("./components/RecoveryScene.svelte") then Mod}
            <Mod.default />
          {:catch}
            {@render still(sceneRecovery)}
          {/await}
        {:else if !webgl}
          {@render still(sceneRecovery)}
        {/if}
      </div>

      <ol class="mt-12 grid gap-10 md:grid-cols-3 md:gap-8">
        <li>
          <div class="t-step">1</div>
          <h3 class="t-h3 mt-4">Plug into the DFU port</h3>
          <p class="mt-2">
            From a mac, restorekit puts the target into DFU for you. On linux or windows, follow the on-screen steps or use a <a href="#dongle" class="lnk">dongle-lite</a>.
          </p>
        </li>
        <li>
          <div class="t-step">2</div>
          <h3 class="t-h3 mt-4">Get the firmware</h3>
          <p class="mt-2">
            restorekit downloads the right firmware from apple and caches it for the next mac. Or hand it your own IPSW.
          </p>
        </li>
        <li>
          <div class="t-step">3</div>
          <h3 class="t-h3 mt-4">Erase and restore, or revive</h3>
          <p class="mt-2">
            Erase and restore wipes it to factory settings. Revive fixes the firmware and keeps your data.
          </p>
        </li>
      </ol>

      <div class="mt-16 grid gap-8 md:grid-cols-[1fr_1.3fr]">
        <div>
          <h3 class="t-h3">Which port is the DFU port?</h3>
          <p class="mt-2 max-w-[44ch]">
            Apple moved it around from model to model. Here are the common ones, or find
            <a href="/dfu-port" class="lnk">the DFU port on every mac</a>.
          </p>
        </div>
        <dl class="t-data">
          {#each dfuPorts as [model, port] (model)}
            <div class="grid grid-cols-[1fr_1.3fr] gap-4 border-t border-copper py-3 last:border-b">
              <dt class="text-silk3">{model}</dt>
              <dd class="text-silk">{port}</dd>
            </div>
          {/each}
        </dl>
      </div>
    </div>
  </section>

  <!-- desktop app -->
  <section id="desktop" class="bg-mask-deep">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2 max-w-3xl">
        The desktop app
      </h2>
      <p class="mt-5 max-w-[62ch]">
        It does everything the cli does. Plug in several macs and it restores them all at once, and it keeps itself updated.
      </p>

      <div class="mt-8 flex flex-wrap items-center gap-3">
        {#each appDownloads as dl (dl.label)}
          <a
            href={dl.url}
            class="btn-line"
          >
            {dl.label}
          </a>
        {/each}
      </div>
      <p class="t-small mt-3">
        {appVersion ? `${appVersion} is the latest release. ` : ""}Older builds are on the
        <a href={RELEASES} class="lnk">releases page</a>.
      </p>

      <div class="mt-12 grid gap-x-8 gap-y-10 md:grid-cols-2">
        <figure class="md:col-span-2">
          <img
            src={shotDevicesDark}
            alt="Devices tab listing connected Macs with serial numbers, ECIDs, modes and ports, with QR and CSV export buttons"
            class="block w-full rounded-lg border border-silk/15"
            width="1720"
            height="1240"
            loading="lazy"
          />
          <figcaption class="mt-3 max-w-[70ch] t-small">
            Serials, ECIDs and modes for every connected mac, with QR codes and CSV export.
          </figcaption>
        </figure>
        <figure>
          <img
            src={shotHistoryDark}
            alt="History tab with a persistent log of every captured and restored Mac"
            class="block w-full rounded-lg border border-silk/15"
            width="1720"
            height="1240"
            loading="lazy"
          />
          <figcaption class="mt-3 t-small">
            A log of every mac you've restored.
          </figcaption>
        </figure>
        <figure>
          <img
            src={shotRestoreLight}
            alt="restorekit restore view in light mode"
            class="block w-full rounded-lg border border-silk/15"
            width="1720"
            height="1240"
            loading="lazy"
          />
          <figcaption class="mt-3 t-small">
            Light mode.
          </figcaption>
        </figure>
      </div>
    </div>
  </section>

  <!-- cli -->
  <section id="cli" class="bg-mask">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-20 md:grid-cols-[1fr_1.25fr]">
      <div>
        <h2 class="t-h2">
          The cli
        </h2>
        <p class="mt-5 max-w-[48ch]">
          Run <code class="text-silk">sudo restorekit restore</code> and bam!
        </p>
        <ul class="mt-6 space-y-3">
          <li class="flex gap-3">{@render pad()}<span><code class="text-silk">--json</code> on most commands</span></li>
          <li class="flex gap-3">{@render pad()}<span>Pick a mac with <code class="text-silk">--ecid</code></span></li>
          
          <li class="flex gap-3">{@render pad()}<span>On windows, <code class="text-silk">restorekit setup-driver</code> binds WinUSB once</span></li>
          <li class="flex gap-3">{@render pad()}<span>On linux, a udev rule lets you skip <code class="text-silk">sudo</code></span></li>
        </ul>
      </div>
      <div class="self-center">
        {@render cmd(
          "cli",
          "sudo restorekit restore",
          `# wipe and reinstall the latest signed macOS
$ sudo restorekit restore

# pick one of several connected Macs
$ sudo restorekit restore --ecid 0xc60a812345678

# no prompts, for scripts
$ sudo restorekit restore --yes

# just flip the target into DFU (macOS hosts)
$ sudo restorekit dfu

# or reboot it
$ sudo restorekit reboot

# every command, most with --json
$ restorekit -h`,
        )}
      </div>
    </div>
  </section>

  <!-- install -->
  <section id="install" class="bg-mask-deep">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2">Install</h2>

      <div class="mt-10 grid gap-10 lg:grid-cols-3 lg:gap-6">
        <div>
          <h3 class="t-h3 mb-3">macOS, with Homebrew</h3>
          {@render cmd(
            "mac",
            "brew trust fcjr/fcjr\nbrew install fcjr/fcjr/restorekit-cli",
            `$ brew trust fcjr/fcjr
$ brew install fcjr/fcjr/restorekit-cli`,
          )}
          <p class="mt-3 t-small">
            For the desktop app, <code class="text-silk2">brew install --cask fcjr/fcjr/restorekit</code>
          </p>
        </div>
        <div>
          <h3 class="t-h3 mb-3">Windows, with Scoop</h3>
          {@render cmd(
            "win",
            "scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr\nscoop install restorekit-cli\nrestorekit setup-driver",
            `$ scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr
$ scoop install restorekit-cli
$ restorekit setup-driver`,
          )}
          <p class="mt-3 t-small">
            The desktop app is on the <a href="{GITHUB}/releases" class="lnk">releases page</a>.
          </p>
        </div>
        <div>
          <h3 class="t-h3 mb-3">Linux, with Snap</h3>
          {@render cmd(
            "linux",
            "sudo snap install restorekit-cli\nsudo restorekit-cli restore",
            `$ sudo snap install restorekit-cli
$ sudo restorekit-cli restore`,
          )}
          <p class="mt-3 t-small">
            For the desktop app, run <code class="text-silk2">sudo snap install restorekit</code>. The .deb and .AppImage builds are on the <a href="{GITHUB}/releases" class="lnk">releases page</a>.
          </p>
        </div>
      </div>

      <p class="mt-10 max-w-[70ch]">
        Building from source compiles a vendored C stack, so read the
        <a href="{GITHUB}/blob/main/docs/building.md" class="lnk">build guide</a> first.
      </p>
    </div>
  </section>

  <!-- library -->
  <section class="bg-mask">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-20 md:grid-cols-[1fr_1.25fr]">
      <div>
        <h2 class="t-h2">
          Use it as a rust crate
        </h2>
        <p class="mt-5 max-w-[48ch]">
          The cli and the app are both built on the restorekit crate, so you can build your own tools on it too. Docs are on <a href="https://docs.rs/restorekit" class="lnk">docs.rs</a>.
        </p>
      </div>
      <div class="self-center rounded-md border border-copper bg-mask-deep">
        <Code
          lang="rust"
          code={`let dev = device::wait(device::Target::One, Duration::from_secs(60))?;
let fw = firmware::resolve(dev.identifier().unwrap(), None)?;
let ipsw = firmware::download(&cache, &fw, &mut |event| {
    // render progress however you like
})?;`}
        />
      </div>
    </div>
  </section>

  <!-- open source -->
  <section id="open-source" class="bg-mask-deep">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2 max-w-3xl">
        Open source
      </h2>
      <p class="mt-5 max-w-[62ch]">
        The software, firmware and dongle hardware are all in <a href={GITHUB} class="lnk">one repo</a>.
      </p>

      <p class="mt-8 max-w-[78ch] t-small">
        Source is Apache-2.0. Linux and windows binaries bundle <a href="https://github.com/libimobiledevice/usbmuxd" class="lnk">usbmuxd</a>, so those builds are GPL-3.0.
      </p>

      <div
        id="sponsor-certification"
        class="mt-12 flex flex-col gap-6 rounded-lg border border-gold/50 bg-mask p-6 md:flex-row md:items-center md:p-8"
      >
        <div class="grow">
          <h3 class="t-h3">
            Sponsor a certified erasure report
          </h3>
          <p class="mt-3 max-w-[62ch]">
            A DFU restore already wipes the drive, but certifications like ADISA need paid lab time. If your org needs certified erasure, sponsor the lab time and the report is public for everyone.
          </p>
        </div>
        <a
          href="mailto:frank@restorekit.org?subject=Sponsoring%20a%20restorekit%20erasure%20certification"
          class="btn-pad shrink-0"
        >
          Email frank@restorekit.org
        </a>
      </div>
    </div>
  </section>

  <!-- sponsor -->
  <section class="bg-mask">
    <div class="mx-auto flex max-w-6xl flex-col items-start gap-6 px-5 py-16 md:flex-row md:items-center">
      <p class="text-silk">restorekit development is sponsored by</p>
      <a href="https://leftshift.com" class="rounded-md bg-silk px-5 py-3 transition-opacity hover:opacity-90">
        <img src={leftshiftLogo} alt="Left Shift Logical" width="150" height="46" loading="lazy" />
      </a>
      <p class="t-small md:ml-auto">
        <a href={SPONSOR} class="lnk">Sponsor restorekit</a>
      </p>
    </div>
  </section>

  <!-- footer -->
  <footer class="border-t border-copper bg-mask-deep">
    <div class="mx-auto flex max-w-6xl flex-col gap-4 px-5 py-8 text-[14.5px] md:flex-row md:items-center">
      <div class="wide font-bold text-silk">restorekit</div>
      <div class="grow"></div>
      <div class="flex flex-wrap gap-x-6 gap-y-2 text-silk2">
        <a href="#dongle" class="hover:text-silk">Reserve dongle-lite</a>
        <a href="/dfu-port" class="hover:text-silk">Find the DFU port</a>
        <a href={GITHUB} class="hover:text-silk">GitHub</a>
        <a href={SPONSOR} class="hover:text-silk">Sponsor</a>
        <a href="{GITHUB}/releases" class="hover:text-silk">Releases</a>
        <a href="https://crates.io/crates/restorekit-cli" class="hover:text-silk">crates.io</a>
        <a href="https://docs.rs/restorekit" class="hover:text-silk">docs.rs</a>
      </div>
    </div>
    <p class="mx-auto max-w-6xl px-5 pb-8 t-small">
      The DFU code is a rust port of Asahi Linux's macvdmtool (also Apache-2.0), thank you to that
      team. Mac and Apple Silicon are trademarks of Apple Inc., Acroname and Cambrionix of their
      respective owners, mentioned here for comparison only.
    </p>
  </footer>
</main>
