<script>
  let { order, iconStyle } = $props();
</script>

{#if iconStyle === "svg"}
  {#if order === "count_first"}
    <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
      <rect x="1" y="10" width="4" height="5" rx="0.5"/>
      <rect x="6" y="6" width="4" height="9" rx="0.5"/>
      <rect x="11" y="2" width="4" height="13" rx="0.5"/>
    </svg>
  {:else}
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="8" cy="8" r="6"/>
      <polyline points="8,4 8,8 11,10"/>
    </svg>
  {/if}
{:else}
  {order === "count_first" ? "#" : "⌚"}
{/if}
