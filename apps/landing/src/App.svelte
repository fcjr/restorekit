<script lang="ts">
  import { onMount } from "svelte";
  import Code from "./components/Code.svelte";
  import shotRestoreDark from "./assets/app-restore-dark.png";
  import shotDevicesDark from "./assets/app-devices-dark.png";
  import shotHistoryDark from "./assets/app-history-dark.png";
  import shotRestoreLight from "./assets/app-restore-light.png";
  import leftshiftLogo from "./assets/leftshift.svg";

  const GITHUB = "https://github.com/fcjr/restorekit";
  const RELEASES = `${GITHUB}/releases`;
  const SPONSOR = "https://github.com/sponsors/fcjr";

  // Direct links to the latest desktop-app builds, resolved from the GitHub
  // API on load; until then (or if it fails) they land on the releases page.
  let appVersion = $state("");
  let appDownloads = $state(
    [
      { label: "macOS · .dmg", pattern: /_aarch64\.dmg$/ },
      { label: "windows · setup.exe", pattern: /_x64-setup\.exe$/ },
      { label: "linux · .deb", pattern: /_amd64\.deb$/ },
      { label: "linux · .AppImage", pattern: /_amd64\.AppImage$/ },
    ].map((d) => ({ ...d, url: RELEASES })),
  );

  onMount(async () => {
    try {
      const res = await fetch("https://api.github.com/repos/fcjr/restorekit/releases/latest");
      if (!res.ok) return;
      const rel: { tag_name?: string; assets?: { name: string; browser_download_url: string }[] } =
        await res.json();
      appVersion = rel.tag_name ?? "";
      appDownloads = appDownloads.map((d) => {
        const asset = rel.assets?.find((a) => d.pattern.test(a.name));
        return asset ? { ...d, url: asset.browser_download_url } : d;
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

  const compareRows: {
    label: string;
    cells: { text: string; tone?: "ok" | "warn" | "dim"; href?: string }[];
  }[] = [
    {
      label: "Price",
      cells: [
        { text: "Free", tone: "ok" },
        { text: "Quote only", tone: "warn" },
        { text: "Quote only + hardware", tone: "warn" },
        { text: "$99/yr per admin · orgs from $600/yr + $3/Mac" },
        { text: "Free" },
        { text: "Free" },
      ],
    },
    {
      label: "Open source",
      cells: [
        { text: "Apache-2.0⁴", tone: "ok" },
        { text: "Proprietary", tone: "dim" },
        { text: "Proprietary", tone: "dim" },
        { text: "Proprietary", tone: "dim" },
        { text: "Proprietary", tone: "dim" },
        { text: "Apache-2.0" },
      ],
    },
    {
      label: "Host platforms",
      cells: [
        { text: "macOS · Linux · Windows", tone: "ok" },
        { text: "macOS · Mac Pro recommended" },
        { text: "Dedicated appliance" },
        { text: "macOS 15+ on Apple Silicon" },
        { text: "macOS" },
        { text: "macOS on Apple Silicon" },
      ],
    },
    {
      label: "Full restore & revive",
      cells: [
        { text: "✓", tone: "ok" },
        { text: "Erase + reinstall" },
        { text: "Erase + reinstall" },
        { text: "Restore paid-only · no revive", tone: "warn" },
        { text: "✓" },
        { text: "—", tone: "dim" },
      ],
    },
    {
      label: "Certified erasure",
      cells: [
        { text: "None yet · sponsor it²", tone: "warn", href: "#sponsor-certification" },
        { text: "ADISA · NIST 800-88 methods", tone: "ok" },
        { text: "ADISA · NIST 800-88", tone: "ok" },
        { text: "—", tone: "dim" },
        { text: "—", tone: "dim" },
        { text: "—", tone: "dim" },
      ],
    },
    {
      label: "Automatic DFU trigger",
      cells: [
        { text: "✓ on Mac hosts¹", tone: "ok" },
        { text: "✓ (Auto DFU)" },
        { text: "No DFU · manual boot to Recovery", tone: "dim" },
        { text: "✓ (post-trial: DFU + reboot only)" },
        { text: "Manual key sequence", tone: "dim" },
        { text: "✓ (trigger only)" },
      ],
    },
    {
      label: "CLI + JSON automation",
      cells: [
        { text: "✓", tone: "ok" },
        { text: "—", tone: "dim" },
        { text: "REST API for reports only", tone: "dim" },
        { text: "—", tone: "dim" },
        { text: "—", tone: "dim" },
        { text: "CLI, no JSON" },
      ],
    },
    {
      label: "Desktop app",
      cells: [
        { text: "✓", tone: "ok" },
        { text: "✓" },
        { text: "Appliance UI" },
        { text: "✓" },
        { text: "✓" },
        { text: "—", tone: "dim" },
      ],
    },
    {
      label: "Multiple targets",
      cells: [
        { text: "Parallel · one process each", tone: "ok" },
        { text: "Via Cambrionix hubs" },
        { text: "One per station", tone: "dim" },
        { text: "Up to 15 via Acroname hubs" },
        { text: "One at a time", tone: "dim" },
        { text: "One target", tone: "dim" },
      ],
    },
    {
      label: "Acroname hub support",
      cells: [
        { text: "—", tone: "dim" },
        { text: "Cambrionix instead³", tone: "dim" },
        { text: "—", tone: "dim" },
        { text: "✓ org tier" },
        { text: "Hubs unsupported", tone: "dim" },
        { text: "—", tone: "dim" },
      ],
    },
    {
      label: "Target Macs",
      cells: [
        { text: "T2 & Apple Silicon", tone: "ok" },
        { text: "T2 & Apple Silicon" },
        { text: "Intel & Apple Silicon" },
        { text: "Apple Silicon only", tone: "dim" },
        { text: "T2 & Apple Silicon" },
        { text: "Apple Silicon only", tone: "dim" },
      ],
    },
  ];

  // The 3D bench is decoration, so three.js only loads when the section is
  // close to the viewport and the browser actually has WebGL.
  let sceneHost = $state<HTMLElement>();
  let showScene = $state(false);
  $effect(() => {
    if (!sceneHost) return;
    let ok = false;
    try {
      const probe = document.createElement("canvas");
      ok = !!(probe.getContext("webgl2") ?? probe.getContext("webgl"));
    } catch {
      ok = false;
    }
    if (!ok) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          showScene = true;
          io.disconnect();
        }
      },
      { rootMargin: "300px" },
    );
    io.observe(sceneHost);
    return () => io.disconnect();
  });

  const dfuPorts = [
    ["14″ / 16″ MacBook Pro", "Left side, port next to MagSafe"],
    ["Mac mini / Studio", "Port closest to the power button"],
    ["MacBook Air / 13″ Pro", "Left side, port closest to the hinge"],
    ["iMac", "Port closest to the edge"],
  ];
</script>

{#snippet eyebrow(text: string, color = "text-fnt")}
  <div class="text-[11px] tracking-[0.18em] uppercase {color} mb-4">{text}</div>
{/snippet}

{#snippet cmd(id: string, lines: string, display: string)}
  <div class="group relative border border-line bg-bar text-left">
    <Code code={display} lang="bash" />
    <button
      class="absolute top-2 right-2 border border-line2 bg-panel px-2.5 py-1 text-[10px] tracking-[0.08em] uppercase text-mut opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:text-amber hover:border-amber"
      onclick={() => copy(id, lines)}
    >
      {copied === id ? "copied" : "copy"}
    </button>
  </div>
{/snippet}

<!-- nav -->
<header class="sticky top-0 z-50 border-b border-line bg-bar/95 backdrop-blur">
  <nav class="mx-auto flex h-14 max-w-6xl items-center gap-6 px-5">
    <a href="#top" class="flex items-center gap-2.5 text-ink">
      <svg viewBox="0 0 32 32" width="17" height="17" aria-hidden="true">
        <rect x="7" y="7" width="18" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="1.8" />
        <path d="M4 16 H10 L12.2 11 L16 21 L19 16 H28" fill="none" stroke="var(--color-amber)" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span class="text-[13px] font-semibold tracking-tight">restorekit</span>
    </a>
    <div class="grow"></div>
    <div class="hidden items-center gap-5 text-[12px] text-mut md:flex">
      <a href="#how" class="hover:text-ink">How it works</a>
      <a href="#desktop" class="hover:text-ink">App</a>
      <a href="#cli" class="hover:text-ink">CLI</a>
      <a href="#compare" class="hover:text-ink">Compare</a>
      <a href="#install" class="hover:text-ink">Install</a>
    </div>
    <a
      href={GITHUB}
      class="border border-line2 px-3 py-1.5 text-[11.5px] text-ink2 transition-colors hover:border-fnt"
    >
      GitHub
    </a>
  </nav>
</header>

<main id="top" class="bg-page">
  <!-- hero -->
  <section class="border-b border-line bg-panel">
    <div class="mx-auto grid max-w-6xl items-center gap-10 px-5 pt-14 pb-14 md:pt-20 lg:grid-cols-[1fr_1.15fr] lg:gap-12">
      <div>
        {@render eyebrow("Open-source Mac recovery · CLI + desktop app", "text-amber")}
        <h1 class="max-w-xl text-[clamp(26px,3.2vw,38px)] font-bold leading-[1.12] tracking-[-0.02em] text-ink">
          Reformat any T2 or Apple Silicon mac from macOS, linux or windows with a single
          command.<span class="caret" aria-hidden="true"></span>
        </h1>
        <p class="mt-5 max-w-lg text-[13.5px] leading-7 text-mut">
          restorekit is a standalone rust library, cli tool, and gui that fully wipes or restores
          a T2 or M series mac without any apple tools. Binaries are statically linked, so there
          is nothing else to install or configure.
        </p>

        <div class="mt-8 flex flex-col items-stretch gap-3 sm:flex-row">
          <a
            href="#install"
            class="bg-amber px-6 py-3 text-center text-[13px] font-semibold text-amber-ink transition-colors hover:bg-amber-hov"
          >
            Install the CLI
          </a>
          <a
            href="#desktop"
            class="border border-line2 px-6 py-3 text-center text-[13px] text-ink2 transition-colors hover:border-fnt"
          >
            Get the desktop app
          </a>
        </div>

        <div class="mt-6 max-w-md">
          {@render cmd("hero", "brew install fcjr/fcjr/restorekit-cli", "$ brew install fcjr/fcjr/restorekit-cli")}
        </div>
      </div>

      <div>
        <!-- signal trace: host to dead mac -->
        <div class="relative mb-3" aria-hidden="true">
          <svg viewBox="0 0 1200 44" class="block w-full" fill="none" preserveAspectRatio="none">
            <path
              class="trace-draw"
              d="M0 22 H500 L530 6 L575 38 L605 22 H1200"
              stroke="var(--color-amber)"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <path
              class="trace-pulse"
              d="M0 22 H500 L530 6 L575 38 L605 22 H1200"
              stroke="var(--color-amber-hov)"
              stroke-width="3"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          <span class="absolute -top-2 left-0 text-[10px] tracking-[0.14em] uppercase text-dim">your machine</span>
          <span class="absolute -top-2 right-0 text-[10px] tracking-[0.14em] uppercase text-dim">the dead mac</span>
        </div>
        <img
          src={shotRestoreDark}
          alt="restorekit desktop app with a MacBook Pro in DFU mode selected, ready to erase and restore"
          class="shot-poweron block w-full rounded-lg border border-line2 shadow-[0_20px_80px_rgba(232,163,61,0.06)]"
          width="1720"
          height="1240"
          fetchpriority="high"
        />
      </div>
    </div>
  </section>

  <!-- fact strip -->
  <section class="border-b border-line bg-bar">
    <div class="mx-auto grid max-w-6xl grid-cols-2 divide-line text-center text-[11px] tracking-[0.1em] uppercase text-mut max-md:gap-y-px md:grid-cols-4 md:divide-x">
      <div class="px-4 py-4">Apache-2.0</div>
      <div class="px-4 py-4">Rust + libirecovery</div>
      <div class="px-4 py-4">Statically linked · zero setup</div>
      <div class="px-4 py-4">Firmware straight from Apple</div>
    </div>
  </section>

  <!-- why -->
  <section class="border-b border-line">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-16 md:grid-cols-[1fr_1.4fr] md:py-20">
      <div>
        {@render eyebrow("Why?")}
        <h2 class="text-[24px] font-semibold tracking-tight text-ink">
          People should be able to repair their mac without needing to own another one!
        </h2>
      </div>
      <div class="space-y-4 text-[13.5px] leading-7 text-mut">
        <p>
          I've worked at a few places where windows was the default machine (including for IT) and
          macs were only issued when requested or required. A lot of times the IT folks are stuck
          carrying a macbook for one job: recovering and resetting the rest of the fleet.
        </p>
        <p>
          I've also seen companies ship a whole new mac to an employee with no apple store nearby
          when a simple reset would have fixed it.
        </p>
        <p class="text-ink2">I think this sucks.</p>
      </div>
    </div>
  </section>

  <!-- how it works -->
  <section id="how" class="border-b border-line bg-panel">
    <div class="mx-auto max-w-6xl px-5 py-16 md:py-20">
      {@render eyebrow("How it works")}
      <h2 class="max-w-lg text-[24px] font-semibold tracking-tight text-ink">
        Plug it in, follow the instructions, and bam!
      </h2>

      <div
        bind:this={sceneHost}
        class="mt-10 h-[280px] overflow-hidden border border-line bg-page md:h-[380px]"
        aria-hidden="true"
      >
        {#if showScene}
          {#await import("./components/RecoveryScene.svelte") then Mod}
            <Mod.default />
          {/await}
        {/if}
      </div>

      <div class="mt-6 grid gap-px border border-line bg-line md:grid-cols-3">
        <div class="bg-panel p-6">
          <div class="text-[11px] tracking-[0.14em] text-amber">01 · CABLE</div>
          <h3 class="mt-3 text-[15px] font-semibold text-ink">Plug into the DFU port</h3>
          <p class="mt-3 text-[12.5px] leading-6 text-mut">
            Cable the target mac to your host's DFU port. If your host is a mac, restorekit will
            automatically set the target machine into DFU mode. On linux or windows it shows you
            the manual steps instead.
          </p>
        </div>
        <div class="bg-panel p-6">
          <div class="text-[11px] tracking-[0.14em] text-amber">02 · FIRMWARE</div>
          <h3 class="mt-3 text-[15px] font-semibold text-ink">It grabs the right firmware</h3>
          <p class="mt-3 text-[12.5px] leading-6 text-mut">
            restorekit detects the mac and downloads the appropriate firmware straight from apple.
            Downloads are checksum verified, resume if interrupted, and get cached for the next
            machine. You can also pin a version or hand it a local IPSW.
          </p>
        </div>
        <div class="bg-panel p-6">
          <div class="text-[11px] tracking-[0.14em] text-amber">03 · RESTORE</div>
          <h3 class="mt-3 text-[15px] font-semibold text-ink">Erase & restore, or revive</h3>
          <p class="mt-3 text-[12.5px] leading-6 text-mut">
            Erase & restore takes the machine back to factory settings. Revive fixes the firmware
            without touching your data. You can watch every step until the mac reboots into Setup
            Assistant.
          </p>
        </div>
      </div>

      <div class="mt-12 grid gap-8 md:grid-cols-[1fr_1.2fr]">
        <div>
          <h3 class="text-[15px] font-semibold text-ink">Which port is the DFU port?</h3>
          <p class="mt-3 max-w-sm text-[12.5px] leading-6 text-mut">
            Unfortunately apple wasn't consistent about which usb port is the DFU (device
            firmware upgrade) port... The desktop app labels it live per machine; here's
            where to find it on the most common macs, or see
            <a href="https://support.apple.com/en-us/120694" class="text-amber hover:text-amber-hov">apple's official list</a>.
          </p>
        </div>
        <div class="border border-line">
          {#each dfuPorts as [model, port], i (model)}
            <div class="grid grid-cols-[1fr_1.4fr] {i > 0 ? 'border-t border-line' : ''}">
              <div class="border-r border-line px-4 py-3 text-[11px] tracking-[0.08em] uppercase text-fnt">{model}</div>
              <div class="px-4 py-3 text-[12.5px] text-ink2">{port}</div>
            </div>
          {/each}
        </div>
      </div>
    </div>
  </section>

  <!-- desktop app -->
  <section id="desktop" class="border-b border-line">
    <div class="mx-auto max-w-6xl px-5 py-16 md:py-20">
      {@render eyebrow("The desktop app")}
      <h2 class="max-w-xl text-[24px] font-semibold tracking-tight text-ink">
        A gui that wraps the restorekit library for point-and-click restores.
      </h2>
      <p class="mt-4 max-w-xl text-[13.5px] leading-7 text-mut">
        Everything the cli does, in a window. Every cabled apple device shows up the moment it
        enumerates, with its mode (DFU, recovery, booted). You approve the helper once on
        macOS, or run the driver setup once on windows, and after that a restore is two clicks.
        Cable up a few macs and it restores them all at once, each in its own process, with a live
        log and progress per machine. It keeps itself updated too.
      </p>

      <div class="mt-7 flex flex-wrap items-center gap-3">
        {#each appDownloads as dl (dl.label)}
          <a
            href={dl.url}
            class="border border-line2 px-4 py-2.5 text-[12px] text-ink2 transition-colors hover:border-amber hover:text-amber"
          >
            ↓ {dl.label}
          </a>
        {/each}
        <span class="text-[11px] text-fnt">
          {appVersion ? `latest · ${appVersion}` : ""}
          <a href={RELEASES} class="text-mut underline underline-offset-4 hover:text-ink2">all releases</a>
        </span>
      </div>

      <div class="mt-10 grid gap-6 md:grid-cols-2">
        <figure class="md:col-span-2">
          <img
            src={shotDevicesDark}
            alt="Devices tab listing connected Macs with serial numbers, ECIDs, modes and ports, with QR and CSV export buttons"
            class="block w-full rounded-lg border border-line2"
            width="1720"
            height="1240"
            loading="lazy"
          />
          <figcaption class="mt-3 text-[11.5px] leading-5 text-fnt">
            The devices tab shows hardware serials, ECIDs and modes for every cabled mac. There are
            QR codes for your asset tracker and CSV export for everything else.
          </figcaption>
        </figure>
        <figure>
          <img
            src={shotHistoryDark}
            alt="History tab with a persistent log of every captured and restored Mac"
            class="block w-full rounded-lg border border-line2"
            width="1720"
            height="1240"
            loading="lazy"
          />
          <figcaption class="mt-3 text-[11.5px] leading-5 text-fnt">
            History is logged automatically, so you have a record of every machine you've captured
            or restored.
          </figcaption>
        </figure>
        <figure>
          <img
            src={shotRestoreLight}
            alt="RestoreKit restore view in light mode"
            class="block w-full rounded-lg border border-line2"
            width="1720"
            height="1240"
            loading="lazy"
          />
          <figcaption class="mt-3 text-[11.5px] leading-5 text-fnt">
            The app follows your system's light or dark appearance.
          </figcaption>
        </figure>
      </div>
    </div>
  </section>

  <!-- cli -->
  <section id="cli" class="border-b border-line bg-panel">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-16 md:grid-cols-[1fr_1.3fr] md:py-20">
      <div>
        {@render eyebrow("The CLI")}
        <h2 class="text-[24px] font-semibold tracking-tight text-ink">
          The whole workflow is one command.
        </h2>
        <p class="mt-4 text-[13.5px] leading-7 text-mut">
          Run <code class="text-ink2">sudo restorekit restore</code> and bam! It detects the mac,
          downloads the right firmware, and restores it to factory settings.
        </p>
        <ul class="mt-6 space-y-3 text-[12.5px] leading-6 text-mut">
          <li class="flex gap-3"><span class="text-amber">→</span> <span>Plays nice with automation: a <code class="text-ink2">--json</code> flag on most commands</span></li>
          <li class="flex gap-3"><span class="text-amber">→</span> <span>Target one of several macs by <code class="text-ink2">--ecid</code> or port</span></li>
          <li class="flex gap-3"><span class="text-amber">→</span> <span>Retries component sends and restores on transport hiccups</span></li>
          <li class="flex gap-3"><span class="text-amber">→</span> <span>Windows: <code class="text-ink2">restorekit setup-driver</code> binds WinUSB once</span></li>
          <li class="flex gap-3"><span class="text-amber">→</span> <span>Linux: ships a udev rule so you can skip <code class="text-ink2">sudo</code></span></li>
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

  <!-- compare -->
  <section id="compare" class="border-b border-line">
    <div class="mx-auto max-w-6xl px-5 py-16 md:py-20">
      {@render eyebrow("Alternatives")}
      <h2 class="max-w-xl text-[24px] font-semibold tracking-tight text-ink">
        How it stacks up against the other tools.
      </h2>

      <div class="mt-10 overflow-x-auto">
        <table class="w-full min-w-[1080px] border-collapse border border-line text-[12px]">
          <thead>
            <tr class="bg-bar text-left">
              <th class="border-b border-line px-4 py-3 text-[10px] font-semibold tracking-[0.1em] uppercase text-fnt"></th>
              <th class="border-b border-l border-line px-4 py-3 text-[12px] font-semibold text-amber">restorekit</th>
              <th class="border-b border-l border-line px-4 py-3 text-[10px] font-semibold tracking-[0.1em] uppercase text-fnt">Blancco Eraser for Apple Devices</th>
              <th class="border-b border-l border-line px-4 py-3 text-[10px] font-semibold tracking-[0.1em] uppercase text-fnt">Device Link for Macs by Ziperase</th>
              <th class="border-b border-l border-line px-4 py-3 text-[10px] font-semibold tracking-[0.1em] uppercase text-fnt">DFU Blaster Pro</th>
              <th class="border-b border-l border-line px-4 py-3 text-[10px] font-semibold tracking-[0.1em] uppercase text-fnt">Apple Configurator / Finder</th>
              <th class="border-b border-l border-line px-4 py-3 text-[10px] font-semibold tracking-[0.1em] uppercase text-fnt">macvdmtool</th>
            </tr>
          </thead>
          <tbody>
            {#each compareRows as row (row.label)}
              <tr class="border-t border-line">
                <td class="px-4 py-3 text-[10px] tracking-[0.08em] uppercase text-fnt">{row.label}</td>
                {#each row.cells as cell, i (i)}
                  <td
                    class="border-l border-line px-4 py-3 {i === 0 ? 'bg-amber-soft' : ''} {cell.tone === 'ok'
                      ? 'text-ok'
                      : cell.tone === 'warn'
                        ? 'text-amber'
                        : cell.tone === 'dim'
                          ? 'text-dim'
                          : 'text-ink2'}"
                  >
                    {#if cell.href}
                      <a
                        href={cell.href}
                        class="underline underline-offset-4 hover:text-amber-hov"
                      >
                        {cell.text}
                      </a>
                    {:else}
                      {cell.text}
                    {/if}
                  </td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <p class="mt-5 max-w-3xl text-[11.5px] leading-6 text-fnt">
        ¹ Triggering DFU over USB-PD needs a T2 or Apple Silicon host. That's a hardware limit and
        it applies to every tool here. On linux and windows you put the target into DFU by hand
        (restorekit shows you the steps), and detection, firmware download, and restore all run
        natively. Vendor pricing and features as published July 2026, check their sites for
        current terms.
      </p>
      <p class="mt-3 max-w-3xl text-[11.5px] leading-6 text-fnt">
        ³ It's already a bit ridiculous that Acroname hubs
        <a
          href="https://acroname.com/store/s106-usbhub-3c-kit"
          class="text-amber hover:text-amber-hov">need a $400 PD-logging license</a
        > before they'll put a Mac into DFU. Cambrionix goes a step further and makes the license
        renewing:
        <a
          href="https://www.cambrionix.com/products/thundersync5-c16-pd"
          class="text-amber hover:text-amber-hov">their DFU-capable hub</a
        > only enters DFU once it's registered in their Connect Premium software, the hub is sold as
        a subscription bundle (£79 a month over three years), and when the term runs out you pay
        again.
      </p>
      <p class="mt-3 max-w-3xl text-[11.5px] leading-6 text-fnt">
        ⁴ The restorekit source is Apache-2.0. A built binary's license depends on what it links:
        macOS builds are Apache-2.0 with LGPL and BSD libraries, while linux and windows builds also
        bundle <a href="https://github.com/libimobiledevice/usbmuxd" class="text-amber hover:text-amber-hov">usbmuxd</a>
        (GPL-3.0), so those release binaries are conveyed as a whole under GPL-3.0. Either way it's
        open source, all the way down.
      </p>
      <div
        id="sponsor-certification"
        class="mt-8 flex max-w-3xl flex-col gap-5 border border-line bg-panel px-6 py-6 md:flex-row md:items-center"
      >
        <div class="grow">
          <h3 class="text-[15px] font-semibold tracking-tight text-ink">
            ² Help make restorekit the first open source tool with certified erasure.
          </h3>
          <p class="mt-2 text-[12.5px] leading-6 text-mut">
            No open source tool has a certified erasure process. Not one. The DFU restore already
            wipes the volume the same way the paid tools do, what's missing is a lab report saying
            so, and certifications like ADISA cost real money. Sponsor the lab time and the whole
            industry gets a free, certified option with your name on it.
          </p>
        </div>
        <a
          href="mailto:frank@restorekit.org?subject=Sponsoring%20a%20restorekit%20erasure%20certification"
          class="shrink-0 bg-amber px-6 py-3 text-center text-[13px] font-semibold text-amber-ink transition-colors hover:bg-amber-hov"
        >
          frank@restorekit.org
        </a>
      </div>
    </div>
  </section>

  <!-- install -->
  <section id="install" class="border-b border-line bg-panel">
    <div class="mx-auto max-w-6xl px-5 py-16 md:py-20">
      {@render eyebrow("Install")}
      <h2 class="text-[24px] font-semibold tracking-tight text-ink">Pick your host.</h2>

      <div class="mt-10 grid gap-6 lg:grid-cols-3">
        <div>
          <h3 class="mb-3 text-[11px] tracking-[0.14em] uppercase text-mut">macOS · Homebrew</h3>
          {@render cmd(
            "mac",
            "brew trust fcjr/fcjr\nbrew install fcjr/fcjr/restorekit-cli",
            `$ brew trust fcjr/fcjr
$ brew install fcjr/fcjr/restorekit-cli`,
          )}
          <p class="mt-3 text-[11.5px] leading-5 text-fnt">
            Desktop app: <code class="text-mut">brew install --cask fcjr/fcjr/restorekit</code>
          </p>
        </div>
        <div>
          <h3 class="mb-3 text-[11px] tracking-[0.14em] uppercase text-mut">Windows · Scoop</h3>
          {@render cmd(
            "win",
            "scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr\nscoop install restorekit-cli\nrestorekit setup-driver",
            `$ scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr
$ scoop install restorekit-cli
$ restorekit setup-driver`,
          )}
          <p class="mt-3 text-[11.5px] leading-5 text-fnt">
            <code class="text-mut">setup-driver</code> binds the WinUSB driver once. Desktop app on the
            <a href="{GITHUB}/releases" class="text-amber hover:text-amber-hov">releases page</a>.
          </p>
        </div>
        <div>
          <h3 class="mb-3 text-[11px] tracking-[0.14em] uppercase text-mut">Linux · releases</h3>
          {@render cmd(
            "linux",
            "sudo restorekit restore",
            `# .deb / .AppImage from GitHub releases
$ sudo restorekit restore`,
          )}
          <p class="mt-3 text-[11.5px] leading-5 text-fnt">
            Skip <code class="text-mut">sudo</code> by installing the bundled
            <a href="{GITHUB}/tree/main/udev" class="text-amber hover:text-amber-hov">udev rule</a>.
            The .deb does it for you.
          </p>
        </div>
      </div>

      <p class="mt-8 text-[12px] leading-6 text-mut">
        All release binaries are statically linked so there is nothing else to install. If you'd
        rather build from source, <code class="text-ink2">cargo install restorekit-cli</code> works
        but compiles a vendored C stack, so read the
        <a href="{GITHUB}/blob/main/docs/building.md" class="text-amber hover:text-amber-hov">build guide</a> first.
      </p>
    </div>
  </section>

  <!-- library -->
  <section class="border-b border-line">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-16 md:grid-cols-[1fr_1.3fr] md:py-20">
      <div>
        {@render eyebrow("As a library")}
        <h2 class="text-[24px] font-semibold tracking-tight text-ink">
          Both the cli and the desktop app are thin shells over the restorekit rust crate.
        </h2>
        <p class="mt-4 text-[13.5px] leading-7 text-mut">
          The <a href="https://docs.rs/restorekit" class="text-amber hover:text-amber-hov">restorekit</a>
          crate exposes the same workflow using a callback based system, so you can build your own
          tooling on top of it.
        </p>
      </div>
      <div class="self-center border border-line bg-bar">
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

  <!-- sponsor -->
  <section class="border-b border-line bg-panel">
    <div class="mx-auto flex max-w-6xl flex-col items-center gap-4 px-5 py-14 text-center">
      {@render eyebrow("Sponsor")}
      <p class="max-w-md text-[14px] leading-7 text-ink2">restorekit development is sponsored by</p>
      <a href="https://leftshift.com" class="opacity-90 transition-opacity hover:opacity-100">
        <img src={leftshiftLogo} alt="Left Shift Logical" width="170" height="52" loading="lazy" />
      </a>
      <p class="text-[11.5px] text-fnt">
        You can help too: <a href={SPONSOR} class="text-amber hover:text-amber-hov">sponsor restorekit</a> or
        <a href={GITHUB} class="text-mut hover:text-ink2 underline underline-offset-4">star and contribute on GitHub</a>.
      </p>
    </div>
  </section>

  <!-- footer -->
  <footer class="bg-bar">
    <div class="mx-auto flex max-w-6xl flex-col gap-4 px-5 py-8 text-[11.5px] text-fnt md:flex-row md:items-center">
      <div class="flex items-center gap-2 text-mut">
        <span class="pulse-dot inline-block h-1.5 w-1.5 bg-amber"></span>
        restorekit · Apache-2.0
      </div>
      <div class="grow"></div>
      <div class="flex flex-wrap gap-5">
        <a href={GITHUB} class="hover:text-ink2">GitHub</a>
        <a href={SPONSOR} class="hover:text-ink2">Sponsor</a>
        <a href="{GITHUB}/releases" class="hover:text-ink2">Releases</a>
        <a href="https://crates.io/crates/restorekit-cli" class="hover:text-ink2">crates.io</a>
        <a href="https://docs.rs/restorekit" class="hover:text-ink2">docs.rs</a>
      </div>
    </div>
    <div class="border-t border-line">
      <p class="mx-auto max-w-6xl px-5 py-4 text-[10.5px] leading-5 text-dim">
        The DFU code is a rust port of Asahi Linux's macvdmtool (also Apache-2.0). DFU Blaster is a
        trademark of Twocanoes Software, Apple Configurator of Apple Inc., and Blancco and Ziperase
        of their respective owners, mentioned here for comparison only.
      </p>
    </div>
  </footer>
</main>
