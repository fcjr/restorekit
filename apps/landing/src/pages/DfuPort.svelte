<script lang="ts">
  import Page from "./Page.svelte";

  const URL = "https://www.restorekit.org/dfu-port";
  const TITLE = "Which port is the DFU port on my mac? · restorekit";
  const DESCRIPTION =
    "Where the DFU port is on every MacBook Pro, MacBook Air, MacBook Neo, iMac, Mac mini, Mac Studio and Mac Pro, and how to get the mac into DFU mode to restore it.";

  // From apple's table at support.apple.com/en-us/120694 (September 2026),
  // reworded so you don't have to picture which side "facing the left side"
  // puts the hinge on.
  const families: { name: string; rows: [string, string][] }[] = [
    {
      name: "MacBook Pro",
      rows: [
        ["14″ with M4 or M5 (not Pro or Max)", "Left side, the port closest to the front"],
        ["Every other Apple Silicon model", "Left side, the port closest to the hinge"],
        ["T2 models", "Left side, the port closest to the front"],
      ],
    },
    {
      name: "MacBook Air",
      rows: [
        ["Apple Silicon, 2025 or later", "Left side, the port closest to the front"],
        ["Every other Apple Silicon model", "Left side, the port closest to the hinge"],
        ["T2 models", "Left side, the port closest to the front"],
      ],
    },
    {
      name: "MacBook Neo",
      rows: [["Every model", "Left side, the port closest to the hinge"]],
    },
    {
      name: "iMac",
      rows: [["Every model", "Looking at the back, the rightmost usb-c port"]],
    },
    {
      name: "Mac mini",
      rows: [
        ["Apple Silicon, 2025 or later", "Looking at the back, the middle usb-c port"],
        ["Every other Apple Silicon model", "Looking at the back, the leftmost usb-c port"],
        ["T2 models", "Looking at the back, the rightmost usb-c port"],
      ],
    },
    {
      name: "Mac Studio",
      rows: [["Every model", "Looking at the back, the rightmost usb-c port"]],
    },
    {
      name: "Mac Pro",
      rows: [
        ["Tower", "On top, the usb-c port farthest from the power button"],
        ["Rack", "On the front, the usb-c port closest to the power button"],
      ],
    },
  ];
</script>

<svelte:head>
  <title>{TITLE}</title>
  <meta name="description" content={DESCRIPTION} />
  <link rel="canonical" href={URL} />
  <meta property="og:title" content={TITLE} />
  <meta property="og:description" content={DESCRIPTION} />
  <meta property="og:type" content="article" />
  <meta property="og:url" content={URL} />
  <meta property="og:image" content="https://www.restorekit.org/og.png" />
  <meta name="twitter:card" content="summary_large_image" />
</svelte:head>

<Page>
  <section class="bg-mask">
    <div class="mx-auto max-w-6xl px-5 pt-14 pb-16 md:pt-20">
      <h1 class="t-display max-w-4xl">Which port is the DFU port on my mac?</h1>
      <p class="t-lead mt-6 max-w-[62ch]">
        Restoring a mac over usb only works through its DFU port. Apple wasn't consistent about which
        port that is, and it moves around between models. Plug into the wrong one and nothing
        happens, so find yours here first.
      </p>
    </div>
  </section>

  <section class="bg-mask-deep">
    <div class="mx-auto max-w-6xl px-5 py-16">
      <div class="grid gap-x-12 gap-y-10 md:grid-cols-2">
        {#each families as fam (fam.name)}
          <div>
            <h2 class="t-h3">{fam.name}</h2>
            <dl class="mt-3 t-data">
              {#each fam.rows as [model, port] (model)}
                <div class="grid grid-cols-[1fr_1.3fr] gap-4 border-t border-copper py-3 last:border-b">
                  <dt class="text-silk3">{model}</dt>
                  <dd class="text-silk">{port}</dd>
                </div>
              {/each}
            </dl>
          </div>
        {/each}
      </div>
      <p class="mt-10 max-w-[70ch] t-small">
        From <a href="https://support.apple.com/en-us/120694" class="lnk">apple's DFU port list</a>.
        Not sure which year your mac is? Apple menu, About This Mac.
      </p>
    </div>
  </section>

  <section class="bg-mask">
    <div class="mx-auto grid max-w-6xl gap-10 px-5 py-20 md:grid-cols-[1fr_1.3fr]">
      <div>
        <h2 class="t-h2">Getting it into DFU mode</h2>
        <p class="mt-5 max-w-[48ch]">
          If the computer you're fixing it from is a T2 or Apple Silicon mac, you don't have to do
          anything. Plug into the DFU port and restorekit flips the target into DFU for you.
        </p>
        <p class="mt-4 max-w-[48ch]">
          From windows or linux, a normal usb-c port can't send the message that does it, so either
          use a <a href="/#dongle" class="lnk">dongle-lite</a> or do it by hand with the button combo.
        </p>
      </div>
      <div class="space-y-8">
        <div>
          <h3 class="t-h3">Apple Silicon laptops, by hand</h3>
          <ol class="mt-3 list-decimal space-y-2 pl-5">
            <li>Plug the cable into the DFU port. Power goes in MagSafe if it has one, nothing else plugged in.</li>
            <li>Hold the power button until the mac turns off.</li>
            <li>Press power once, then right away hold left Control, left Option, right Shift and power together.</li>
            <li>After about 10 seconds let go of the three keys but keep holding power.</li>
            <li>Keep holding power for up to 10 more seconds. The screen stays black, that's DFU.</li>
          </ol>
        </div>
        <div>
          <h3 class="t-h3">Apple Silicon desktops, by hand</h3>
          <ol class="mt-3 list-decimal space-y-2 pl-5">
            <li>Plug the cable into the DFU port and unplug the mac from power.</li>
            <li>Hold the power button, plug power back in, and keep holding for up to 10 seconds.</li>
          </ol>
        </div>
        <p class="t-small">
          The timing is fiddly, it can take a few tries. T2 macs use a different combo, the
          <a href="https://theapplewiki.com/wiki/DFU_Mode" class="lnk">apple wiki</a> has it.
          Use a usb-c cable that does data and charging (apple says not a Thunderbolt 3 cable) and
          go straight into both machines, no hubs.
        </p>
      </div>
    </div>
  </section>

  <section class="bg-mask-deep">
    <div class="mx-auto max-w-6xl px-5 py-20">
      <h2 class="t-h2 max-w-3xl">Then restore it</h2>
      <p class="mt-5 max-w-[62ch]">
        restorekit finds the mac, downloads the right firmware from apple and restores it. It runs on
        macOS, linux and windows, so you don't need a second mac. It's free and open source.
      </p>
      <div class="mt-8 flex flex-col gap-3 sm:flex-row">
        <a href="/#install" class="btn-pad">Install restorekit</a>
        <a href="/#dongle" class="btn-line">Reserve a dongle</a>
      </div>
    </div>
  </section>
</Page>
