<script>
  let {
    allCompletions,
    completionIndex = $bindable(0),
    MAX_COMPLETIONS,
    historyArgs,
    onselectcompletion,
    onapplycompletion,
  } = $props();
</script>

{#if allCompletions.length > 0}
  {@const winStart = Math.max(0, Math.min(completionIndex - Math.floor(MAX_COMPLETIONS / 2), allCompletions.length - MAX_COMPLETIONS))}
  {@const visible = allCompletions.slice(winStart, winStart + MAX_COMPLETIONS)}
  <div class="results">
    {#each visible as comp, i}
      {@const globalIdx = winStart + i}
      <div
        class="item"
        class:selected={globalIdx === completionIndex}
        onmouseenter={() => onselectcompletion(globalIdx)}
        onclick={() => { onselectcompletion(globalIdx); onapplycompletion(); }}
        role="option"
        aria-selected={globalIdx === completionIndex}
      >
        <span class="item-name completion-path" class:is-dir={comp.endsWith('/')}>{comp}</span>
        <div class="item-right">
          {#if allCompletions.length > MAX_COMPLETIONS}
            <span class="completion-count">{globalIdx + 1}/{allCompletions.length}</span>
          {/if}
          {#if historyArgs.includes(comp)}
            <span class="item-source" data-source="History">History</span>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .results {
    border-top: 1px solid var(--color-surface, #313244);
    overflow-y: auto;
    padding-bottom: 8px;
  }

  .item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    cursor: pointer;
    color: var(--color-text, #cdd6f4);
  }

  .item.selected { background: var(--color-surface, #313244); }

  .item-name {
    font-size: var(--font-size, 14px);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .completion-path {
    font-size: calc(var(--font-size, 14px) - 1px);
    color: var(--color-text, #cdd6f4);
    font-family: monospace;
  }

  .completion-path.is-dir { color: var(--color-blue, #89b4fa); }

  .item-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .completion-count {
    font-size: calc(var(--font-size, 14px) - 4px);
    color: var(--color-overlay, #45475a);
    flex-shrink: 0;
  }

  .item-source {
    font-size: calc(var(--font-size, 14px) - 3px);
    color: var(--color-muted, #585b70);
    text-transform: lowercase;
  }

  :global(.item-source[data-source="History"]) { color: var(--color-red, #f38ba8); }
</style>
