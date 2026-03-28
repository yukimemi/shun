<script>
  let { mode, iconStyle } = $props();
</script>

{#if iconStyle === "svg"}
  {#if mode === "fuzzy"}
    <!-- 3 wavy lines = fuzzy/approximate -->
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
      <path d="M2 4.5 C4 3 6 6 8 4.5 C10 3 12 6 14 4.5"/>
      <path d="M2 8   C4 6.5 6 9.5 8 8   C10 6.5 12 9.5 14 8"/>
      <path d="M2 11.5 C4 10 6 13 8 11.5 C10 10 12 13 14 11.5"/>
    </svg>
  {:else if mode === "exact"}
    <!-- 1 straight line = exact/literal -->
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
      <line x1="2" y1="8" x2="14" y2="8"/>
    </svg>
  {:else if mode === "migemo"}
    <!-- あ = migemo -->
    <svg width="14" height="14" viewBox="0 0 16 16">
      <text x="8" y="13" text-anchor="middle" font-size="13" fill="currentColor" font-family="sans-serif">あ</text>
    </svg>
  {:else if mode === "fuzzy_migemo"}
    <!-- ≋あ = fuzzy + migemo -->
    <svg width="20" height="14" viewBox="0 0 22 16">
      <path d="M1 4.5 C2.5 3 4 6 5.5 4.5 C7 3 8.5 6 10 4.5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
      <path d="M1 8   C2.5 6.5 4 9.5 5.5 8 C7 6.5 8.5 9.5 10 8" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
      <path d="M1 11.5 C2.5 10 4 13 5.5 11.5 C7 10 8.5 13 10 11.5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
      <text x="17" y="13" text-anchor="middle" font-size="12" fill="currentColor" font-family="sans-serif">あ</text>
    </svg>
  {:else}
    <!-- ―あ = exact + migemo -->
    <svg width="20" height="14" viewBox="0 0 22 16">
      <line x1="1" y1="8" x2="10" y2="8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
      <text x="17" y="13" text-anchor="middle" font-size="12" fill="currentColor" font-family="sans-serif">あ</text>
    </svg>
  {/if}
{:else}
  {mode === "fuzzy" ? "≋" : mode === "exact" ? "―" : mode === "migemo" ? "あ" : mode === "fuzzy_migemo" ? "≋あ" : "―あ"}
{/if}
