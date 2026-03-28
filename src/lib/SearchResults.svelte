<script>
  import { canHaveArgs } from "$lib/utils.js";

  let {
    filtered,
    filteredSlash,
    selectedIndex = $bindable(0),
    slashResult,
    MAX_ITEMS,
    onrunslash,
    onlaunch,
    onopenconfig,
  } = $props();
</script>

{#if !slashResult && filteredSlash.length > 0}
  <div class="results">
    {#each filteredSlash as cmd, i}
      <div
        class="item"
        class:selected={i === selectedIndex}
        onmouseenter={() => (selectedIndex = i)}
        onclick={() => onrunslash(cmd)}
        role="option"
        aria-selected={i === selectedIndex}
      >
        <span class="item-name slash-name">{cmd.name}</span>
        <span class="item-source">{cmd.description}</span>
      </div>
    {/each}
  </div>
{:else if !slashResult && filtered.length > 0}
  {@const winStart = Math.max(0, Math.min(selectedIndex - Math.floor(MAX_ITEMS / 2), filtered.length - MAX_ITEMS))}
  {@const visible = filtered.slice(winStart, winStart + MAX_ITEMS)}
  <div class="results">
    {#each visible as item, i}
      {@const globalIdx = winStart + i}
      <div
        class="item"
        class:selected={globalIdx === selectedIndex}
        class:warning-item={item.source === "Warning"}
        onmouseenter={() => (selectedIndex = globalIdx)}
        onclick={() => item.source === "Warning" ? onopenconfig(item.path) : onlaunch(item)}
        role="option"
        aria-selected={globalIdx === selectedIndex}
      >
        <span class="item-name" class:scrolling={globalIdx === selectedIndex} data-warning={item.source === "Warning" ? "true" : null}>{item.source === "Warning" ? "⚠ " : ""}{item.name}</span>
        <div class="item-right">
          {#if item.source === "Warning"}
            <span class="item-warning-error">{item._warning_error}</span>
          {:else}
            {#if canHaveArgs(item)}
              <span class="item-tab-hint">tab</span>
            {/if}
            {#if filtered.length > MAX_ITEMS}
              <span class="completion-count">{globalIdx + 1}/{filtered.length}</span>
            {/if}
            <span class="item-source" data-source={item.source}>{item.source}</span>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{:else if !slashResult}
  <div class="results">
    <div class="empty">No results</div>
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

  .item-name.scrolling {
    overflow: auto;
    text-overflow: clip;
    scrollbar-width: none;
  }

  .item-name.scrolling::-webkit-scrollbar { display: none; }

  .item-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .item-tab-hint {
    font-size: calc(var(--font-size, 14px) - 4px);
    color: var(--color-overlay, #45475a);
    background: var(--color-surface, #313244);
    padding: 1px 5px;
    border-radius: 3px;
  }

  .item-source {
    font-size: calc(var(--font-size, 14px) - 3px);
    color: var(--color-muted, #585b70);
    text-transform: lowercase;
  }

  .completion-count {
    font-size: calc(var(--font-size, 14px) - 4px);
    color: var(--color-overlay, #45475a);
    flex-shrink: 0;
  }

  .empty {
    padding: 16px 20px;
    color: var(--color-muted, #585b70);
    font-size: var(--font-size, 14px);
  }

  .slash-name {
    color: var(--color-purple, #cba6f7);
    font-family: monospace;
    font-size: var(--font-size, 14px);
  }

  .item-warning-error {
    color: var(--color-red, #f38ba8);
    font-size: calc(var(--font-size, 14px) - 2px);
    opacity: 0.85;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .warning-item .item-name { flex: 0 0 auto; }
  .warning-item .item-right {
    flex: 1;
    min-width: 0;
    margin-left: 12px;
    justify-content: flex-end;
  }
  .warning-item .item-warning-error { flex: 1; min-width: 0; }

  :global(.item-source[data-source="Url"])     { color: var(--color-blue, #89b4fa); }
  :global(.item-source[data-source="Path"])    { color: var(--color-green, #a6e3a1); }
  :global(.item-source[data-source="History"]) { color: var(--color-red, #f38ba8); }
  :global(.item-name[data-warning="true"])     { color: var(--color-red, #f38ba8); }
</style>
