<script>
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  const WINDOW_WIDTH = 620;
  const INPUT_HEIGHT = 52;
  const ITEM_HEIGHT = 38;
  const BORDER_HEIGHT = 1;
  const RESULTS_PADDING = 8;
  const MAX_ITEMS = 8;

  const win = getCurrentWindow();

  let query = $state("");
  let allItems = $state([]);
  let filtered = $state([]);
  let selectedIndex = $state(0);
  let inputEl = $state(null);

  function resizeWindow(itemCount) {
    const count = Math.min(itemCount, MAX_ITEMS);
    const resultsHeight = BORDER_HEIGHT + (count > 0 ? count : 1) * ITEM_HEIGHT + RESULTS_PADDING;
    const height = INPUT_HEIGHT + resultsHeight;
    win.setSize(new LogicalSize(WINDOW_WIDTH, height));
  }

  onMount(async () => {
    await listen("show-launcher", async () => {
      query = "";
      setTimeout(() => inputEl?.focus(), 30);
    });

    allItems = await invoke("get_apps");
  });

  function onKeydown(e) {
    if (e.key === "Escape") {
      getCurrentWindow().hide();
    } else if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1);
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filtered[selectedIndex]) {
        launchItem(filtered[selectedIndex]);
      }
    }
  }

  $effect(() => {
    invoke("search_items", { query }).then((results) => {
      filtered = results;
      selectedIndex = 0;
      resizeWindow(results.length);
    });
  });

  async function launchItem(item) {
    await invoke("launch_item", { item });
    getCurrentWindow().hide();
  }

  function focusInput(el) {
    el.focus();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <div class="launcher">
    <input
      type="text"
      class="search"
      placeholder="Type to search..."
      bind:value={query}
      bind:this={inputEl}
      use:focusInput
      autocomplete="off"
      spellcheck="false"
    />
    {#if filtered.length > 0}
      <div class="results">
        {#each filtered.slice(0, 8) as item, i}
          <div
            class="item"
            class:selected={i === selectedIndex}
            onmouseenter={() => (selectedIndex = i)}
            onclick={() => launchItem(item)}
            role="option"
            aria-selected={i === selectedIndex}
          >
            <span class="item-name">{item.name}</span>
            <span class="item-source">{item.source}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="results">
        <div class="empty">No results</div>
      </div>
    {/if}
  </div>
</main>

<style>
  * {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
  }

  main {
    width: 100vw;
    height: 100vh;
    background: transparent;
  }

  .launcher {
    width: 100%;
    height: 100%;
    background: #1e1e2e;
    overflow: hidden;
  }

  .search {
    width: 100%;
    padding: 16px 20px;
    font-size: 18px;
    background: transparent;
    border: none;
    outline: none;
    color: #cdd6f4;
    font-family: inherit;
  }

  .search::placeholder {
    color: #585b70;
  }

  .results {
    border-top: 1px solid #313244;
    overflow-y: auto;
    padding-bottom: 8px;
  }

  .item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    cursor: pointer;
    color: #cdd6f4;
  }

  .item.selected {
    background: #313244;
  }

  .item-name {
    font-size: 14px;
  }

  .item-source {
    font-size: 11px;
    color: #585b70;
    text-transform: lowercase;
  }

  .empty {
    padding: 16px 20px;
    color: #585b70;
    font-size: 14px;
  }
</style>
