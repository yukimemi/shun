<script>
  let { highlighted, content, el = $bindable(null) } = $props();
</script>

<div class="preview-panel" bind:this={el}>
  {#if highlighted}
    {@html highlighted}
  {:else}
    <pre class="preview-content">{content}</pre>
  {/if}
</div>

<style>
  .preview-panel {
    flex: 1;
    min-height: 0; /* flex アイテムの min-height: auto を上書き → overflow-y が効く */
    overflow-y: auto;
    background: var(--color-bg, #1e1e2e);
    opacity: var(--opacity, 1);
    border-left: 1px solid var(--color-surface1, #313244);
    scrollbar-width: thin;
    scrollbar-color: var(--color-surface1, #313244) transparent;
  }

  .preview-panel::-webkit-scrollbar {
    width: 4px;
  }

  .preview-panel::-webkit-scrollbar-track {
    background: transparent;
  }

  .preview-panel::-webkit-scrollbar-thumb {
    background: var(--color-surface1, #313244);
    border-radius: 2px;
  }

  .preview-panel::-webkit-scrollbar-thumb:hover {
    background: var(--color-overlay0, #6c7086);
  }

  /* Shiki が出力する pre > code のスタイル調整 */
  .preview-panel :global(pre) {
    margin: 0;
    padding: 10px 12px;
    font-family: monospace;
    font-size: calc(var(--font-size, 14px) - 1px);
    white-space: pre-wrap;
    word-break: break-all;
    background: transparent !important;
    overflow: visible !important; /* 親 .preview-panel がスクロールを担う */
  }

  .preview-content {
    padding: 10px 12px;
    margin: 0;
    font-family: monospace;
    font-size: calc(var(--font-size, 14px) - 1px);
    color: var(--color-subtext0, #a6adc8);
    white-space: pre-wrap;
    word-break: break-all;
    line-height: 1.5;
  }
</style>
