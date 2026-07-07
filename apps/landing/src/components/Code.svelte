<script lang="ts">
  import { highlight } from "../lib/highlight";

  let { code, lang = "bash" as "bash" | "rust" } = $props();

  let html = $state("");
  $effect(() => {
    let live = true;
    highlight(code, lang)
      .then((h) => {
        if (live) html = h;
      })
      .catch(() => {
        /* plain text stays */
      });
    return () => {
      live = false;
    };
  });
</script>

<div class="codeblock">
  {#if html}
    {@html html}
  {:else}
    <pre>{code}</pre>
  {/if}
</div>
