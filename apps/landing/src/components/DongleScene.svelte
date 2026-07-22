<script lang="ts">
  import { Canvas } from "@threlte/core";
  import Scene from "./DongleSceneInner.svelte";

  // Port labels tracking the projected positions of the real J1/J2 connectors.
  let host = $state<{ x: number; y: number }>();
  let target = $state<{ x: number; y: number }>();
  function onports(h: { x: number; y: number }, t: { x: number; y: number }) {
    host = h;
    target = t;
  }
</script>

<div class="relative h-full w-full">
  <Canvas>
    <Scene {onports} />
  </Canvas>
  {#if host && target}
    {#each [
      { p: host, label: "host port" },
      { p: target, label: "target port" },
    ] as { p, label } (label)}
      <span
        class="pointer-events-none absolute z-10 flex -translate-x-1/2 flex-col items-center text-[10px] tracking-[0.14em] uppercase text-mut"
        style="left: {Math.min(Math.max(p.x, 0.07), 0.93) * 100}%; top: {p.y * 100}%"
      >
        <span class="h-10 w-px bg-line2"></span>
        <span class="bg-page/85 px-1.5 py-0.5 whitespace-nowrap">{label}</span>
      </span>
    {/each}
  {/if}
</div>
