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
  const MAX_COMPLETIONS = 6;

  const win = getCurrentWindow();

  // モード: "search" | "args"
  let mode = $state("search");
  let query = $state("");
  let extraArgs = $state("");
  let argItem = $state(null);
  let filtered = $state([]);
  let selectedIndex = $state(0);
  let inputEl = $state(null);
  let argsEl = $state(null);

  // ghost text & 補完
  let completionPrefix = $state("");  // Rust が返す prefix (パス以外の部分)
  let allCompletions = $state([]);    // 全補完候補
  let completionIndex = $state(0);   // 選択中インデックス

  // 現在の ghost suffix (表示用)
  let ghostSuffix = $derived(() => {
    if (!allCompletions.length) return "";
    const candidate = allCompletions[completionIndex];
    const partial = extraArgs.slice(completionPrefix.length);
    if (candidate.toLowerCase().startsWith(partial.toLowerCase())) {
      return candidate.slice(partial.length);
    }
    return "";
  });

  function resizeForSearch(itemCount) {
    const count = Math.min(itemCount, MAX_ITEMS);
    const h = INPUT_HEIGHT + BORDER_HEIGHT + (count > 0 ? count : 1) * ITEM_HEIGHT + RESULTS_PADDING;
    win.setSize(new LogicalSize(WINDOW_WIDTH, h));
  }

  function resizeForArgs(completionCount) {
    const count = Math.min(completionCount, MAX_COMPLETIONS);
    if (count === 0) {
      win.setSize(new LogicalSize(WINDOW_WIDTH, INPUT_HEIGHT));
    } else {
      const h = INPUT_HEIGHT + BORDER_HEIGHT + count * ITEM_HEIGHT + RESULTS_PADDING;
      win.setSize(new LogicalSize(WINDOW_WIDTH, h));
    }
  }

  function resetToSearch() {
    mode = "search";
    argItem = null;
    extraArgs = "";
    completionPrefix = "";
    allCompletions = [];
    completionIndex = 0;
    setTimeout(() => inputEl?.focus(), 10);
  }

  function selectCompletion(idx) {
    completionIndex = idx;
  }

  function acceptWord() {
    if (!ghostSuffix()) return;
    const suffix = ghostSuffix();
    const slashIdx = suffix.indexOf("/");
    if (slashIdx === -1) {
      extraArgs = extraArgs + suffix;
    } else {
      extraArgs = extraArgs + suffix.slice(0, slashIdx + 1);
    }
  }

  function acceptLine() {
    if (!ghostSuffix()) return;
    extraArgs = extraArgs + ghostSuffix();
    allCompletions = [];
  }

  function applySelectedCompletion() {
    if (!allCompletions.length) return;
    const candidate = allCompletions[completionIndex];
    const partial = extraArgs.slice(completionPrefix.length);
    if (candidate.toLowerCase().startsWith(partial.toLowerCase())) {
      extraArgs = completionPrefix + candidate;
    }
    allCompletions = [];
  }

  onMount(async () => {
    await listen("show-launcher", async () => {
      mode = "search";
      argItem = null;
      extraArgs = "";
      query = "";
      completionPrefix = "";
      allCompletions = [];
      completionIndex = 0;
      setTimeout(() => inputEl?.focus(), 30);
    });
  });

  function onKeydown(e) {
    if (mode === "args") {
      if (e.key === "Escape") {
        e.preventDefault();
        if (allCompletions.length > 0) {
          allCompletions = [];
        } else {
          resetToSearch();
        }
      } else if (e.key === "Enter") {
        e.preventDefault();
        if (allCompletions.length > 0) {
          const candidate = allCompletions[completionIndex];
          applySelectedCompletion();
          // ファイル（ディレクトリでない）なら即起動
          if (!candidate.endsWith('/') && argItem) {
            launchItem(argItem, extraArgs);
          }
        } else if (argItem) {
          launchItem(argItem, extraArgs);
        }
      } else if (e.key === "Tab") {
        e.preventDefault();
        if (allCompletions.length > 0) {
          applySelectedCompletion();
        }
      } else if (e.ctrlKey && e.key === "e") {
        e.preventDefault();
        acceptLine();
      } else if (e.ctrlKey && e.key === "f") {
        e.preventDefault();
        acceptWord();
      } else if (e.ctrlKey && e.key === "n") {
        e.preventDefault();
        if (allCompletions.length > 0) {
          completionIndex = (completionIndex + 1) % allCompletions.length;
        }
      } else if (e.ctrlKey && e.key === "p") {
        e.preventDefault();
        if (allCompletions.length > 0) {
          completionIndex = (completionIndex - 1 + allCompletions.length) % allCompletions.length;
        }
      }
      return;
    }

    // search モード
    if (e.key === "Escape") {
      win.hide();
    } else if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1);
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === "Tab") {
      e.preventDefault();
      const item = filtered[selectedIndex];
      if (item?.allow_extra_args) {
        argItem = item;
        mode = "args";
        win.setSize(new LogicalSize(WINDOW_WIDTH, INPUT_HEIGHT));
        setTimeout(() => argsEl?.focus(), 10);
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filtered[selectedIndex]) {
        launchItem(filtered[selectedIndex], null);
      }
    }
  }

  // search モード: クエリで絞り込み
  $effect(() => {
    invoke("search_items", { query }).then((results) => {
      filtered = results;
      selectedIndex = 0;
      resizeForSearch(results.length);
    });
  });

  // args モード: extraArgs 変化で補完を更新
  $effect(() => {
    if (mode !== "args") return;
    const input = extraArgs;
    if (!input) {
      completionPrefix = "";
      allCompletions = [];
      resizeForArgs(0);
      return;
    }
    invoke("complete_path", {
      input,
      completionType: argItem?.completion ?? "path",
      completionList: argItem?.completion_list ?? [],
      completionCommand: argItem?.completion_command ?? null,
    }).then((result) => {
      completionPrefix = result.prefix;
      allCompletions = result.completions;
      completionIndex = 0;
      resizeForArgs(result.completions.length);
    });
  });

  async function launchItem(item, args) {
    const extraArgsList = args ? args.trim().split(/\s+/).filter(Boolean) : [];
    await invoke("launch_item", { item, extraArgs: extraArgsList });
    win.hide();
    resetToSearch();
  }

  function focusInput(el) {
    el.focus();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <div class="launcher">
    {#if mode === "search"}
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
        {@const winStart = Math.max(0, Math.min(selectedIndex - Math.floor(MAX_ITEMS / 2), filtered.length - MAX_ITEMS))}
        {@const visible = filtered.slice(winStart, winStart + MAX_ITEMS)}
        <div class="results">
          {#each visible as item, i}
            {@const globalIdx = winStart + i}
            <div
              class="item"
              class:selected={globalIdx === selectedIndex}
              onmouseenter={() => (selectedIndex = globalIdx)}
              onclick={() => launchItem(item, null)}
              role="option"
              aria-selected={globalIdx === selectedIndex}
            >
              <span class="item-name">{item.name}</span>
              <div class="item-right">
                {#if item.allow_extra_args}
                  <span class="item-tab-hint">tab</span>
                {/if}
                {#if filtered.length > MAX_ITEMS}
                  <span class="completion-count">{globalIdx + 1}/{filtered.length}</span>
                {/if}
                <span class="item-source">{item.source}</span>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="results">
          <div class="empty">No results</div>
        </div>
      {/if}
    {:else}
      <!-- args モード -->
      <div class="args-bar">
        <span class="args-app-name">{argItem?.name}</span>
        <span class="args-sep">›</span>
        <div class="args-input-wrap">
          <div class="ghost-overlay" aria-hidden="true">
            <span class="ghost-typed">{extraArgs}</span><span class="ghost-text">{ghostSuffix()}</span>
          </div>
          <input
            type="text"
            class="args-input"
            placeholder={extraArgs ? "" : "extra args..."}
            bind:value={extraArgs}
            bind:this={argsEl}
            autocomplete="off"
            spellcheck="false"
          />
        </div>
      </div>
      {#if allCompletions.length > 0}
        {@const winStart = Math.max(0, Math.min(completionIndex - Math.floor(MAX_COMPLETIONS / 2), allCompletions.length - MAX_COMPLETIONS))}
        {@const visible = allCompletions.slice(winStart, winStart + MAX_COMPLETIONS)}
        <div class="results">
          {#each visible as comp, i}
            {@const globalIdx = winStart + i}
            <div
              class="item"
              class:selected={globalIdx === completionIndex}
              onmouseenter={() => selectCompletion(globalIdx)}
              onclick={() => { selectCompletion(globalIdx); applySelectedCompletion(); }}
              role="option"
              aria-selected={globalIdx === completionIndex}
            >
              <span class="item-name completion-path">{comp}</span>
              {#if allCompletions.length > MAX_COMPLETIONS}
                <span class="completion-count">{globalIdx + 1}/{allCompletions.length}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
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

  /* args モード */
  .args-bar {
    display: flex;
    align-items: center;
    padding: 0 20px;
    height: 52px;
    gap: 10px;
  }

  .args-app-name {
    font-size: 18px;
    color: #89b4fa;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .args-sep {
    font-size: 18px;
    color: #45475a;
    flex-shrink: 0;
  }

  .args-input-wrap {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
  }

  .ghost-overlay {
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    display: flex;
    align-items: center;
    pointer-events: none;
    font-size: 18px;
    font-family: inherit;
    white-space: pre;
    overflow: hidden;
  }

  .ghost-typed { color: transparent; }
  .ghost-text  { color: #45475a; }

  .args-input {
    position: relative;
    z-index: 1;
    width: 100%;
    font-size: 18px;
    background: transparent;
    border: none;
    outline: none;
    color: #cdd6f4;
    font-family: inherit;
  }

  .args-input::placeholder { color: #585b70; }

  /* 共通リスト */
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

  .item.selected { background: #313244; }

  .item-name { font-size: 14px; }

  .completion-path {
    font-size: 13px;
    color: #a6e3a1;
    font-family: monospace;
  }

  .completion-count {
    font-size: 10px;
    color: #45475a;
    flex-shrink: 0;
  }

  .item-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .item-tab-hint {
    font-size: 10px;
    color: #45475a;
    background: #313244;
    padding: 1px 5px;
    border-radius: 3px;
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
