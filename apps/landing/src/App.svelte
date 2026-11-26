<script lang="ts">
  import { onMount } from "svelte";
  import Code from "./components/Code.svelte";
  import Page from "./pages/Page.svelte";
  import { GITHUB, RELEASES, SPONSOR } from "./lib/links";
  import shotRestoreDark from "./assets/app-restore-dark.png";
  import shotDevicesDark from "./assets/app-devices-dark.png";
  import shotHistoryDark from "./assets/app-history-dark.png";
  import shotRestoreLight from "./assets/app-restore-light.png";
  import leftshiftLogo from "./assets/leftshift.svg";
  import sceneDongle from "./assets/scene-dongle.webp";
  import sceneRecovery from "./assets/scene-recovery.webp";

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

  // Install commands per OS. The hero shows the visitor's OS with the others
  // one click away, and the install section shows all three.
  type Host = "mac" | "windows" | "linux";
  const hosts: { id: Host; label: string; via: string; lines: string; dl: number }[] = [
    {
      id: "mac",
      label: "macOS",
      via: "Homebrew",
      lines: "brew trust fcjr/fcjr\nbrew install fcjr/fcjr/restorekit-cli",
      dl: 0,
    },
    {
      id: "windows",
      label: "Windows",
      via: "Scoop",
      lines:
        "scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr\nscoop install restorekit-cli\nrestorekit setup-driver",
      dl: 1,
    },
    {
      id: "linux",
      label: "Linux",
      via: "Snap",
      lines: "sudo snap install restorekit-cli\nsudo restorekit-cli restore",
      dl: 2,
    },
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

  // Supported targets and hosts shown right under the hero.
  const supportedDevices: [string, string][] = [
    [
      "Apple Silicon macs",
      "Every M1 through M6 model, including MacBook Air, MacBook Pro, MacBook Neo, iMac, Mac mini, Mac Studio and Mac Pro. Full macOS erase and restore, revive and obliterate.",
    ],
    [
      "T2 Intel macs",
      "Every Intel mac with an Apple T2 chip. Restores and revives bridgeOS over DFU.",
    ],
    [
      "iPhones and iPads",
      "Every Lightning and USB-C model apple still signs iOS or iPadOS firmware for. Restores from recovery or DFU, and takes running devices into recovery over USB.",
    ],
    [
      "Host platforms",
      "macOS, linux and windows, with no Apple Configurator, iTunes or Finder. Windows sets up WinUSB automatically; linux ships a udev rule and bundles usbmuxd.",
    ],
  ];

  // dongle-pro vs the DFU-capable USB hubs people usually end up pricing out.
  const hubCols = ["dongle-pro", "Acroname USBHub 3c", "Cambrionix ThunderSync5 C16 PD"];
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

  // dongle-pro preorder-interest form -> POST /api/reserve -> D1.
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

  const withPrompt = (lines: string) => lines.split("\n").map((l) => `$ ${l}`).join("\n");
</script>

{#snippet cmd(id: string, lines: string, display = withPrompt(lines))}
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

<Page>
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
      class="relative mx-auto grid max-w-6xl items-center gap-10 px-5 pt-10 pb-12 md:pt-12 md:pb-14 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)] lg:gap-12"
    >
      <div>
        <h1 class="t-display">Reformat modern Apple products from any platform.</h1>
        <p class="t-lead mt-5 max-w-[52ch]">
          Wipe, revive or obliterate T2 and Apple Silicon macs, iPhones and iPads from macOS, linux or windows, without Apple Configurator or iTunes. It's free and open source.
        </p>

        <div class="mt-6 flex flex-col gap-3 sm:flex-row">
          <a href={appDownloads[heroHost.dl].url} class="btn-pad">Download for {heroHost.label}</a>
          <a href="#install" class="btn-line">Other ways to install</a>
        </div>

        <div class="mt-6 max-w-md">
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
          {@render cmd("hero", heroHost.lines)}
        </div>
      </div>

      <div>
        <img
          src={shotRestoreDark}
          alt="restorekit desktop app restoring a MacBook Pro, 42% through sending the filesystem, with two more macs queued"
          class="block w-full rounded-lg border border-silk/15 shadow-[0_30px_60px_-20px_rgba(4,20,14,0.8)] lg:w-[135%] lg:max-w-none"
          width="1720"
          height="1240"
          fetchpriority="high"
        />
        <a
          href="#dongle"
          class="group mt-6 flex items-center gap-4 rounded-lg border border-copper-hi bg-mask-deep/80 p-3 pr-5 transition-colors hover:border-gold"
        >
          <span class="h-14 w-28 shrink-0 overflow-hidden rounded-[5px] bg-mask-deep">
            <img src={sceneDongle} alt="" class="h-full w-full scale-110 object-cover object-[50%_52%]" width="1208" height="956" />
          </span>
          <span class="t-data">
            <strong class="font-semibold text-silk">No second mac for DFU.</strong>
            dongle-pro puts a mac into DFU from any computer, even windows.
            <span class="whitespace-nowrap text-gold group-hover:text-gold-hi">Join the batch 2 list</span>
          </span>
        </a>
      </div>
    </div>
  </section>

  <!-- dongle-pro -->
  <section id="dongle" class="bg-mask-deep">
    <div class="mx-auto grid max-w-6xl items-start gap-10 px-5 py-20 lg:grid-cols-[1fr_1.3fr]">
      <div>
        <h2 class="t-h2">Automatic DFU for macs from any computer. Yes, even windows.</h2>
        <p class="mt-5 max-w-[56ch]">
          Putting a mac into DFU takes a USB-PD message that normal PC ports can't send, so you've always needed a second mac. dongle-pro sends it for you, passes USB 3 straight through to the mac, and exposes apple's serial console so you can see why a restore failed. (iPhones and iPads don't need one, they enter recovery over regular USB.)
        </p>
        <p class="mt-4 max-w-[56ch]">
          Buying one from me is what keeps restorekit going, so I'd love it if you did. But it's
          open hardware, so if you'd rather fire up the soldering iron, the
          <a href="{GITHUB}/tree/main/hardware/dongle-pro-v2" class="lnk">schematics and fab files</a>
          are all in the repo. I won't be offended. Much.
        </p>

        <dl class="mt-7 grid grid-cols-[auto_1fr] gap-x-6 gap-y-2.5 t-data">
          <dt class="text-silk3">Ports</dt>
          <dd class="text-silk">Two usb-c, one to your computer and one to the mac</dd>
          <dt class="text-silk3">Chip</dt>
          <dd class="text-silk">CH32X035 running rust firmware</dd>
          <dt class="text-silk3">USB</dt>
          <dd class="text-silk">USB 3 passthrough to the mac</dd>
          <dt class="text-silk3">Size</dt>
          <dd class="text-silk">48 × 21 mm</dd>
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
      <h2 class="t-h2 max-w-3xl">Compared to DFU hubs</h2>
      <p class="mt-5 max-w-[62ch]">
        <a href="https://acroname.com/store/s106-usbhub-3c-kit" class="lnk">Acroname</a>
        and
        <a href="https://www.cambrionix.com/products/thundersync5-c16-pd" class="lnk">Cambrionix</a>
        make hubs that work at scale, for racks of macs. They've also had DFU hardware to
        themselves for years, and they price like it. Acroname charges $400 for the software
        license that switches DFU on. Cambrionix's DFU hub is £1,999. Triggering DFU is one USB-PD
        message. It shouldn't cost that much. dongle-pro does it from the computer you already
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

  <!-- supported devices & restore modes -->
  <section id="devices" class="bg-mask-deep">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2 max-w-3xl">What it works with</h2>
      <p class="mt-5 max-w-[62ch]">
        Everything runs on a self-contained stack built with rust,
        <a href="https://github.com/libimobiledevice/libirecovery" class="lnk">libirecovery</a>
        and
        <a href="https://github.com/libimobiledevice/idevicerestore" class="lnk">idevicerestore</a>,
        straight over USB from macOS, linux or windows.
      </p>

      <div class="mt-10 grid gap-12 lg:grid-cols-[1.2fr_1fr]">
        <div>
          <h3 class="t-h3">Devices</h3>
          <dl class="mt-4 t-data">
            {#each supportedDevices as [label, detail] (label)}
              <div class="grid gap-x-6 gap-y-1 border-t border-copper py-3.5 last:border-b sm:grid-cols-[11rem_1fr]">
                <dt class="text-silk">{label}</dt>
                <dd class="text-silk2">{detail}</dd>
              </div>
            {/each}
          </dl>
        </div>

        <div>
          <h3 class="t-h3">Restore modes</h3>
          <ul class="mt-4 space-y-4">
            <li class="flex gap-3">
              {@render pad()}
              <span>
                <strong class="font-semibold text-silk">Erase and restore.</strong>
                Wipes the device back to factory settings and installs the latest signed macOS, iOS or iPadOS, a pinned version, or a local IPSW.
              </span>
            </li>
            <li class="flex gap-3">
              {@render pad()}
              <span>
                <strong class="font-semibold text-silk">Revive.</strong>
                Reinstalls firmware without erasing user data, to recover a device stuck after a failed update.
              </span>
            </li>
            <li class="flex gap-3">
              {@render pad()}
              <span>
                <strong class="font-semibold text-silk">Obliterate.</strong>
                Destroys the Secure Enclave encryption key and stops without reinstalling the OS. Takes about 30 seconds instead of 15 to 30 minutes, for fast decommissioning wipes.
              </span>
            </li>
            <li class="flex gap-3">
              {@render pad()}
              <span>
                <strong class="font-semibold text-silk">Verified wipe logs.</strong>
                Checks the device's <code class="text-silk">format_effaceable_storage</code> checkpoint on every erase or obliterate, logs the verdict and checkpoints to history, and exports CSV.
              </span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </section>

  <!-- why -->
  <section class="bg-mask">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-20 md:grid-cols-[1fr_1.2fr]">
      <h2 class="t-h2">People should be able to repair their mac without needing to own another one!</h2>
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
          It started with macs, and now it handles iPhones and iPads too, without Apple Configurator
          or iTunes. The software is free and Apache-2.0, and it'll stay that way.
          <a href="#dongle" class="lnk">dongle-pro</a> is the part you can buy, and selling it pays for my time on the rest.
        </p>
      </div>
    </div>
  </section>

  <!-- how it works -->
  <section id="how" class="bg-mask-deep">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2">Plug it in, follow the instructions, and bam!</h2>

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
          <h3 class="t-h3 mt-4">Put it in DFU or recovery</h3>
          <p class="mt-2">
            For a mac, plug into its <a href="/dfu-port" class="lnk">DFU port</a>. A mac host triggers DFU automatically; on linux or windows, follow the on-screen combo or use a <a href="#dongle" class="lnk">dongle-pro</a>. For an iPhone or iPad, plug into any port and restorekit puts a running device into recovery for you (tap Trust once, or pass a supervision identity to skip it).
          </p>
        </li>
        <li>
          <div class="t-step">2</div>
          <h3 class="t-h3 mt-4">Get the firmware</h3>
          <p class="mt-2">
            restorekit downloads the latest signed macOS, bridgeOS, iOS or iPadOS firmware from apple and caches it locally so you only pull each IPSW once. You can also pin an OS version or pass a local IPSW.
          </p>
        </li>
        <li>
          <div class="t-step">3</div>
          <h3 class="t-h3 mt-4">Erase, revive or obliterate</h3>
          <p class="mt-2">
            Erase and restore wipes it to factory settings. Revive reinstalls firmware and keeps your data. Obliterate shreds the encryption key in about 30 seconds without reinstalling the OS.
          </p>
        </li>
      </ol>
    </div>
  </section>

  <!-- desktop app -->
  <section id="desktop" class="bg-mask">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2 max-w-3xl">The desktop app</h2>
      <p class="mt-5 max-w-[62ch]">
        It wraps everything the cli does in one standalone macOS, linux and windows app. Connect macs, iPhones and iPads, restore several at once in parallel against one shared firmware cache, manage dongles and update their firmware over USB, and let the app update itself.
      </p>

      <div class="mt-8 flex flex-wrap items-center gap-3">
        {#each appDownloads as dl (dl.label)}
          <a href={dl.url} class="btn-line">{dl.label}</a>
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
            height="792"
            loading="lazy"
          />
          <figcaption class="mt-3 max-w-[70ch] t-small">
            Serials, ECIDs and modes for every connected mac, iPhone and iPad, with inline QR codes, all-seen history and CSV export.
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
          <figcaption class="mt-3 t-small">A persistent log of every restore and wipe, with encryption-key checkpoints.</figcaption>
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
          <figcaption class="mt-3 t-small">Light mode.</figcaption>
        </figure>
      </div>
    </div>
  </section>

  <!-- cli -->
  <section id="cli" class="bg-mask-deep">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-20 md:grid-cols-[1fr_1.25fr]">
      <div>
        <h2 class="t-h2">The cli</h2>
        <p class="mt-5 max-w-[48ch]">
          Run <code class="text-silk">sudo restorekit restore</code> and bam!
        </p>
        <ul class="mt-6 space-y-3">
          <li class="flex gap-3">{@render pad()}<span><code class="text-silk">--json</code> on most commands, plus <code class="text-silk">list --watch</code> for live monitoring</span></li>
          <li class="flex gap-3">{@render pad()}<span>Pick any device with <code class="text-silk">--ecid</code>, pin firmware with <code class="text-silk">--os-version</code> or <code class="text-silk">--ipsw</code></span></li>
          <li class="flex gap-3">{@render pad()}<span>Take a running iPhone or iPad to recovery with <code class="text-silk">recovery</code>, or skip Trust with <code class="text-silk">--supervisor</code></span></li>
          <li class="flex gap-3">{@render pad()}<span>Stream a mac's debug UART with <code class="text-silk">serial</code>, or manage dongles with <code class="text-silk">dongle update</code> and <code class="text-silk">replug</code></span></li>
          <li class="flex gap-3">{@render pad()}<span>On windows, <code class="text-silk">restorekit setup-driver</code> binds WinUSB once. On linux, a udev rule lets you skip <code class="text-silk">sudo</code>.</span></li>
        </ul>
      </div>
      <div class="self-center">
        {@render cmd(
          "cli",
          "sudo restorekit restore",
          `# wipe and reinstall the latest signed OS
$ sudo restorekit restore

# revive without erasing user data
$ sudo restorekit revive

# shred the encryption key in ~30s without reinstalling the OS
$ sudo restorekit obliterate --yes

# restore a specific mac, iPhone or iPad by ECID
$ sudo restorekit restore --ecid 0xc60a812345678

# take a supervised iPhone or iPad to recovery without tapping Trust
$ restorekit recovery --supervisor identity.p12

# watch connected devices, or trigger mac DFU (mac host or dongle)
$ restorekit list --watch
$ sudo restorekit dfu

# every command, most with --json
$ restorekit -h`,
        )}
      </div>
    </div>
  </section>

  <!-- install -->
  <section id="install" class="bg-mask">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2">Install</h2>

      <div class="mt-10 grid gap-10 lg:grid-cols-3 lg:gap-6">
        {#each hosts as h (h.id)}
          <div>
            <h3 class="t-h3 mb-3">{h.label}, with {h.via}</h3>
            {@render cmd(h.id, h.lines)}
            <p class="mt-3 t-small">
              {#if h.id === "mac"}
                For the desktop app, <code class="text-silk2">brew install --cask fcjr/fcjr/restorekit</code> or grab the <a href={appDownloads[0].url} class="lnk">.dmg</a>.
              {:else if h.id === "windows"}
                No iTunes or Apple Devices needed. For the desktop app, grab the <a href={appDownloads[1].url} class="lnk">installer</a>.
              {:else}
                For the desktop app, run <code class="text-silk2">sudo snap install restorekit</code>, or
                grab the <a href={appDownloads[2].url} class="lnk">.deb</a> or
                <a href={appDownloads[3].url} class="lnk">.AppImage</a>.
              {/if}
            </p>
          </div>
        {/each}
      </div>

      <p class="mt-10 max-w-[70ch]">
        Building from source compiles a vendored C stack, so read the
        <a href="{GITHUB}/blob/main/docs/building.md" class="lnk">build guide</a> first.
      </p>
    </div>
  </section>

  <!-- library -->
  <section class="bg-mask-deep">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-20 md:grid-cols-[1fr_1.25fr]">
      <div>
        <h2 class="t-h2">Use it as a rust crate</h2>
        <p class="mt-5 max-w-[48ch]">
          The desktop app and cli are thin shells over the restorekit crate. Device discovery, DFU triggering, iOS recovery entry, firmware caching and restore, revive and obliterate are all exposed through a callback API. Docs are on <a href="https://docs.rs/restorekit" class="lnk">docs.rs</a>.
        </p>
      </div>
      <div class="self-center rounded-md border border-copper bg-mask-deep">
        <Code
          lang="rust"
          code={`let dev = device::wait(device::Target::One, Duration::from_secs(60))?;
let fw = firmware::resolve(dev.identifier().unwrap(), None)?;
let ipsw = firmware::download(&cache, &fw, &mut |event| {
    // download progress, cache hits, checksum verification
})?;
restore::restore(&ipsw, dev.ecid.unwrap(), Some(&cache), Mode::Erase, false, &mut |event| {
    // restore steps, wipe checkpoints, obliteration verdict
})?;`}
        />
      </div>
    </div>
  </section>

  <!-- open source -->
  <section id="open-source" class="bg-mask">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2 max-w-3xl">Open source</h2>
      <p class="mt-5 max-w-[62ch]">
        The software, firmware and dongle hardware are all in <a href={GITHUB} class="lnk">one repo</a>.
      </p>

      <p class="mt-3 max-w-[78ch] t-small">
        Source is Apache-2.0. Linux and windows binaries bundle <a href="https://github.com/libimobiledevice/usbmuxd" class="lnk">usbmuxd</a>, so those builds are GPL-3.0.
      </p>

      <div class="mt-10 flex flex-col items-start gap-6 md:flex-row md:items-center">
        <p class="text-silk">restorekit development is sponsored by</p>
        <a href="https://leftshift.com" class="rounded-md bg-silk px-5 py-3 transition-opacity hover:opacity-90">
          <img src={leftshiftLogo} alt="Left Shift Logical" width="150" height="46" loading="lazy" />
        </a>
        <a href={SPONSOR} class="lnk t-small md:ml-auto">Sponsor restorekit</a>
      </div>

      <div
        id="sponsor-certification"
        class="mt-12 flex flex-col gap-6 rounded-lg border border-gold/50 bg-mask-deep p-6 md:flex-row md:items-center md:p-8"
      >
        <div class="grow">
          <h3 class="t-h3">Sponsor a certified erasure report</h3>
          <p class="mt-3 max-w-[62ch]">
            Erase and obliterate already shred the Secure Enclave encryption key and log the device's <code class="text-silk">format_effaceable_storage</code> checkpoint, but certifications like ADISA need paid lab time. If your org needs certified erasure, sponsor the lab time and the report is public for everyone.
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
</Page>
