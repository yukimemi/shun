<script>
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { onMount } from "svelte";
  import { firstSepIdx, isPathQuery, matchKey } from "$lib/utils.js";

  let WINDOW_WIDTH = $state(620);
  const INPUT_HEIGHT = 52;
  const ITEM_HEIGHT = 40;
  const BORDER_HEIGHT = 1;
  const RESULTS_PADDING = 8;
  let MAX_ITEMS = $state(8);
  let MAX_COMPLETIONS = $state(6);

  const win = getCurrentWindow();

  // keybindings (config から取得、デフォルトはハードコード値) ※matchKey は $lib/utils.js
  let keybindings = $state({
    next:        "Ctrl+n",
    prev:        "Ctrl+p",
    confirm:     "Enter",
    arg_mode:    "Tab",
    accept_word: "Ctrl+f",
    accept_line: "Ctrl+e",
    delete_word: "Ctrl+w",
    delete_line: "Ctrl+u",
    run_query:   "Shift+Enter",
    close:       "Escape",
    delete_item: "Ctrl+d",
  });

  function makePathItem(p) {
    return { name: p, path: p, args: [], workdir: null,
             source: "Path", completion: "none", completion_list: [], completion_command: null };
  }

  function canHaveArgs(item) {
    return item?.source !== "Url" && item?.source !== "Path" && item?.source !== "History";
  }

  let appVersion = $state("");
  let updateVersion = $state("");

  let SLASH_COMMANDS = $derived([
    { name: "/exit",    description: "Quit app" },
    { name: "/config",  description: "Open config file" },
    { name: "/history", description: "Open history file" },
    { name: "/rescan",  description: "Rescan apps" },
    { name: "/version", description: appVersion ? `v${appVersion}` : "Show version" },
    { name: "/update",  description: updateVersion ? `Update to v${updateVersion}` : "Check for updates" },
  ]);

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
  let lastArgsGhost = $state("");     // 前回使った args の ghost
  let historyArgs = $state([]);       // args 履歴（sort_order 順）

  // 現在の ghost suffix (args モード用)
  // allCompletions はフル文字列なので extraArgs との差分をそのまま返す
  let ghostSuffix = $derived(() => {
    if (!allCompletions.length) return "";
    const candidate = allCompletions[completionIndex];
    if (candidate.toLowerCase().startsWith(extraArgs.toLowerCase())) {
      return candidate.slice(extraArgs.length);
    }
    return "";
  });

  // search モード: 選択中候補の path がクエリのプレフィックスならghost表示
  let searchGhostSuffix = $derived(() => {
    if (!query || filtered.length === 0) return "";
    const candidate = filtered[selectedIndex]?.path ?? filtered[0]?.path ?? "";
    if (candidate.toLowerCase().startsWith(query.toLowerCase())) {
      return candidate.slice(query.length);
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
    lastArgsGhost = "";
    historyArgs = [];
    setTimeout(() => inputEl?.focus(), 10);
  }

  function selectCompletion(idx) {
    completionIndex = idx;
  }

  function acceptWord() {
    if (extraArgs === "" && lastArgsGhost) {
      const sep = firstSepIdx(lastArgsGhost);
      extraArgs = sep === -1 ? lastArgsGhost : lastArgsGhost.slice(0, sep + 1);
      lastArgsGhost = "";
      return;
    }
    if (!ghostSuffix()) return;
    const suffix = ghostSuffix();
    const sep = firstSepIdx(suffix);
    extraArgs = extraArgs + (sep === -1 ? suffix : suffix.slice(0, sep + 1));
  }

  function deleteWord() {
    const el = argsEl;
    if (!el) return;
    const pos = el.selectionStart ?? extraArgs.length;
    const val = extraArgs;
    let i = pos - 1;
    while (i >= 0 && val[i] === " ") i--;
    while (i >= 0 && val[i] !== " " && val[i] !== "/") i--;
    extraArgs = val.slice(0, i + 1) + val.slice(pos);
    setTimeout(() => { el.selectionStart = el.selectionEnd = i + 1; }, 0);
  }

  function deleteLine() {
    const el = argsEl;
    if (!el) return;
    const pos = el.selectionStart ?? extraArgs.length;
    extraArgs = extraArgs.slice(pos);
    setTimeout(() => { el.selectionStart = el.selectionEnd = 0; }, 0);
  }

  function acceptLine() {
    if (extraArgs === "" && lastArgsGhost) {
      extraArgs = lastArgsGhost;
      lastArgsGhost = "";
      return;
    }
    if (!ghostSuffix()) return;
    extraArgs = extraArgs + ghostSuffix();
    allCompletions = [];
  }

  function applySelectedCompletion() {
    if (!allCompletions.length) return;
    const candidate = allCompletions[completionIndex];
    if (candidate.toLowerCase().startsWith(extraArgs.toLowerCase())) {
      extraArgs = candidate;
    }
    allCompletions = [];
  }

  onMount(async () => {
    const cfg = await invoke("get_config");
    if (cfg?.keybindings) keybindings = { ...keybindings, ...cfg.keybindings };
    if (cfg?.window_width)    WINDOW_WIDTH    = cfg.window_width;
    if (cfg?.max_items)       MAX_ITEMS       = cfg.max_items;
    if (cfg?.max_completions) MAX_COMPLETIONS = cfg.max_completions;
    appVersion = await getVersion();

    await listen("update-available", (event) => {
      updateVersion = event.payload;
    });

    await listen("update-progress", (event) => {
      const { downloaded, total } = event.payload;
      const mb = (downloaded / 1024 / 1024).toFixed(1);
      if (total) {
        const pct = Math.round((downloaded / total) * 100);
        const totalMb = (total / 1024 / 1024).toFixed(1);
        query = `/update — ${pct}% (${mb} / ${totalMb} MB)`;
      } else {
        query = `/update — ${mb} MB downloaded`;
      }
    });

    await listen("update-log", (event) => {
      const { line } = event.payload;
      if (line.trim()) query = `/update — ${line.trim()}`;
    });

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
      if (matchKey(e, keybindings.close)) {
        e.preventDefault();
        if (allCompletions.length > 0) {
          allCompletions = [];
        } else {
          resetToSearch();
        }
      } else if (matchKey(e, keybindings.confirm)) {
        e.preventDefault();
        if (allCompletions.length > 0) {
          const candidate = allCompletions[completionIndex];
          applySelectedCompletion();
          if (!candidate.endsWith('/') && argItem) {
            launchItem(argItem, extraArgs);
          }
        } else if (argItem) {
          launchItem(argItem, extraArgs);
        }
      } else if (matchKey(e, keybindings.arg_mode)) {
        e.preventDefault();
        if (allCompletions.length > 0) {
          applySelectedCompletion();
        }
      } else if (matchKey(e, keybindings.accept_line)) {
        e.preventDefault();
        acceptLine();
      } else if (matchKey(e, keybindings.accept_word)) {
        e.preventDefault();
        acceptWord();
      } else if (matchKey(e, keybindings.delete_word)) {
        e.preventDefault();
        deleteWord();
      } else if (matchKey(e, keybindings.delete_line)) {
        e.preventDefault();
        deleteLine();
      } else if (matchKey(e, keybindings.next)) {
        e.preventDefault();
        if (allCompletions.length > 0) {
          completionIndex = (completionIndex + 1) % allCompletions.length;
        }
      } else if (matchKey(e, keybindings.prev)) {
        e.preventDefault();
        if (allCompletions.length > 0) {
          completionIndex = (completionIndex - 1 + allCompletions.length) % allCompletions.length;
        }
      }
      return;
    }

    // search モード
    if (matchKey(e, keybindings.close)) {
      win.hide();
    } else if (e.key === "ArrowDown" || matchKey(e, keybindings.next)) {
      e.preventDefault();
      const len = filteredSlash.length > 0 ? filteredSlash.length : filtered.length;
      selectedIndex = Math.min(selectedIndex + 1, len - 1);
    } else if (e.key === "ArrowUp" || matchKey(e, keybindings.prev)) {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (matchKey(e, keybindings.accept_word)) {
      e.preventDefault();
      if (searchGhostSuffix()) {
        const suffix = searchGhostSuffix();
        const sep = firstSepIdx(suffix);
        query = sep === -1 ? query + suffix : query + suffix.slice(0, sep + 1);
      }
    } else if (matchKey(e, keybindings.accept_line)) {
      e.preventDefault();
      if (searchGhostSuffix()) {
        query = query + searchGhostSuffix();
      }
    } else if (matchKey(e, keybindings.arg_mode)) {
      e.preventDefault();
      if (isPathQuery(query) && filtered[selectedIndex]) {
        query = filtered[selectedIndex].path;
      } else {
        const item = filtered[selectedIndex];
        if (item?.source === "History") {
          // History アイテム (path\targs) → base exe でargs modeに入り、既存argsはghost textで提示
          const baseName = item.name.split(" › ")[0];
          argItem = { ...item, name: baseName, args: [], source: "ScanDir", history_key: null };
          extraArgs = "";
          mode = "args";
          lastArgsGhost = item.args.join(" ");
          historyArgs = [];
          win.setSize(new LogicalSize(WINDOW_WIDTH, INPUT_HEIGHT));
          setTimeout(() => argsEl?.focus(), 10);
          invoke("get_args_history", { path: item.path }).then((candidates) => {
            historyArgs = candidates;
          });
        } else if (canHaveArgs(item)) {
          argItem = item;
          mode = "args";
          lastArgsGhost = "";
          historyArgs = [];
          win.setSize(new LogicalSize(WINDOW_WIDTH, INPUT_HEIGHT));
          setTimeout(() => argsEl?.focus(), 10);
          invoke("get_args_history", { path: item.path }).then((candidates) => {
            historyArgs = candidates;
            if (candidates.length > 0) lastArgsGhost = candidates[0];
          });
        }
      }
    } else if (matchKey(e, keybindings.run_query)) {
      e.preventDefault();
      if (filteredSlash.length > 0) {
        runSlashCommand(filteredSlash[selectedIndex] ?? filteredSlash[0]);
      } else if (query && filtered.length > 0) {
        // Run the typed query as the base (non-history) item
        const baseItem = filtered.find((item) => item.source !== "History");
        launchItem(baseItem ?? filtered[selectedIndex], null);
      }
    } else if (matchKey(e, keybindings.confirm)) {
      e.preventDefault();
      if (filteredSlash.length > 0) {
        runSlashCommand(filteredSlash[selectedIndex] ?? filteredSlash[0]);
      } else if (filtered[selectedIndex]) {
        const item = isPathQuery(query)
          ? makePathItem(query)
          : filtered[selectedIndex];
        launchItem(item, null);
      }
    } else if (matchKey(e, keybindings.delete_item)) {
      e.preventDefault();
      const item = filtered[selectedIndex];
      if (item?.source === "History") {
        invoke("delete_history_item", { key: item.history_key ?? item.path }).then(() => {
          invoke("rescan");
        });
        filtered = filtered.filter((_, i) => i !== selectedIndex);
        selectedIndex = Math.min(selectedIndex, filtered.length - 1);
        resizeForSearch(filtered.length);
      }
    }
  }

  // 選択アイテムの name が truncate されている場合に scrollLeft でスクロール
  $effect(() => {
    const item = filtered[selectedIndex]; // 依存として登録
    if (mode !== "search" || !item) return;

    const el = document.querySelector(".item-name.scrolling");
    if (!el || el.scrollWidth <= el.clientWidth) return;

    const maxScroll = el.scrollWidth - el.clientWidth;
    let pos = 0;
    let direction = 1;
    let pause = 20; // 開始時に少し待つ

    const id = setInterval(() => {
      if (pause > 0) { pause--; return; }
      pos += direction * 2;
      if (pos >= maxScroll) { pos = maxScroll; direction = -1; pause = 20; }
      else if (pos <= 0)    { pos = 0;         direction =  1; pause = 20; }
      el.scrollLeft = pos;
    }, 16);

    return () => { clearInterval(id); if (el) el.scrollLeft = 0; };
  });

  // MAX_ITEMS / MAX_COMPLETIONS が変わったときにウィンドウサイズを再計算
  $effect(() => {
    const _mi = MAX_ITEMS;       // 依存として登録
    const _mc = MAX_COMPLETIONS; // 依存として登録
    if (mode === "search") {
      resizeForSearch(filtered.length);
    } else {
      resizeForArgs(allCompletions.length);
    }
  });

  // search モード: クエリで絞り込み
  $effect(() => {
    if (query.startsWith("/")) {
      filtered = [];
      selectedIndex = 0;
      resizeForSearch(filteredSlash.length);
      return;
    }
    if (query.startsWith("http://") || query.startsWith("https://")) {
      invoke("search_items", { query }).then((results) => {
        // history 候補を先頭に、入力中の URL が候補にない場合は末尾に追加
        const typed = { name: query, path: query, args: [], workdir: null, source: "Url", completion: "none", completion_list: [], completion_command: null };
        const hasExact = results.some((r) => r.path === query);
        filtered = hasExact ? results : [...results, typed];
        selectedIndex = 0;
        resizeForSearch(filtered.length);
      });
      return;
    }
    if (isPathQuery(query)) {
      invoke("complete_path", { input: query, completionType: "path", completionList: [], completionCommand: null, workdir: null })
        .then((result) => {
          filtered = result.completions.length > 0
            ? result.completions.map(makePathItem)
            : [makePathItem(query)];
          selectedIndex = 0;
          resizeForSearch(filtered.length);
        });
      return;
    }
    invoke("search_items", { query }).then((results) => {
      filtered = results;
      selectedIndex = 0;
      resizeForSearch(results.length);
    });
  });

  // args モード: extraArgs / historyArgs 変化で補完を更新
  // allCompletions はすべて「extraArgs に直接セットできるフル文字列」で統一
  $effect(() => {
    if (mode !== "args") return;
    const input = extraArgs;

    // historyArgs を入力でフィルタ（前方一致・大文字小文字無視）
    const filteredHistory = historyArgs.filter((h) =>
      h.toLowerCase().startsWith(input.toLowerCase())
    );

    if (!input) {
      // 未入力: history のみ表示
      completionPrefix = "";
      allCompletions = filteredHistory;
      completionIndex = 0;
      resizeForArgs(filteredHistory.length);
      return;
    }

    // 入力あり: path/command 補完と history をマージ
    invoke("complete_path", {
      input,
      completionType: argItem?.completion ?? "path",
      completionList: argItem?.completion_list ?? [],
      completionCommand: argItem?.completion_command ?? null,
      workdir: argItem?.workdir ?? null,
    }).then((result) => {
      // path 補完はフル文字列に展開（prefix + item）
      const pathFull = result.completions.map((c) => result.prefix + c);
      // history と重複するものを除外して後ろに追加
      const deduped = pathFull.filter((p) => !filteredHistory.includes(p));
      completionPrefix = "";
      allCompletions = [...filteredHistory, ...deduped];
      completionIndex = 0;
      resizeForArgs(allCompletions.length);
    });
  });

  // スラッシュコマンドの絞り込み
  let filteredSlash = $derived(
    query.startsWith("/")
      ? SLASH_COMMANDS.filter((c) => c.name.startsWith(query.toLowerCase()))
      : []
  );

  async function runSlashCommand(cmd) {
    if (cmd.name === "/version") {
      query = `/version — v${appVersion}`;
      return;
    }
    if (cmd.name === "/update") {
      query = updateVersion ? `/update — starting download...` : `/update — checking...`;
      try {
        await invoke("install_update");
        // ここに到達 = 更新なし（更新ありの場合は app.restart() で戻ってこない）
        query = `/update — already up to date`;
        setTimeout(() => { query = ""; }, 2000);
      } catch (e) {
        query = `/update — error: ${e}`;
        setTimeout(() => { query = ""; }, 3000);
      }
      return;
    }
    win.hide();
    resetToSearch();
    if (cmd.name === "/exit") {
      await invoke("exit_app");
    } else if (cmd.name === "/config") {
      await invoke("open_config");
    } else if (cmd.name === "/history") {
      await invoke("open_history");
    } else if (cmd.name === "/rescan") {
      await invoke("rescan");
    }
  }

  async function launchItem(item, args) {
    const extraArgsList = args ? args.trim().split(/\s+/).filter(Boolean) : [];
    try {
      await invoke("launch_item", { item, extraArgs: extraArgsList });
    } catch (e) {
      console.error("launch failed:", e);
    }
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
      <div class="search-wrap">
        {#if searchGhostSuffix()}
          <div class="ghost-overlay search-ghost" aria-hidden="true">
            <span class="ghost-typed">{query}</span><span class="ghost-text">{searchGhostSuffix()}</span>
          </div>
        {/if}
        <input
          type="text"
          class="search"
          placeholder={updateVersion ? `Update available: v${updateVersion} — /update` : "Type to search..."}
          bind:value={query}
          bind:this={inputEl}
          use:focusInput
          autocomplete="off"
          spellcheck="false"
        />
      </div>
      {#if filteredSlash.length > 0}
        <div class="results">
          {#each filteredSlash as cmd, i}
            <div
              class="item"
              class:selected={i === selectedIndex}
              onmouseenter={() => (selectedIndex = i)}
              onclick={() => runSlashCommand(cmd)}
              role="option"
              aria-selected={i === selectedIndex}
            >
              <span class="item-name slash-name">{cmd.name}</span>
              <span class="item-source">{cmd.description}</span>
            </div>
          {/each}
        </div>
      {:else if filtered.length > 0}
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
              <span class="item-name" class:scrolling={globalIdx === selectedIndex}>{item.name}</span>
              <div class="item-right">
                {#if canHaveArgs(item)}
                  <span class="item-tab-hint">tab</span>
                {/if}
                {#if filtered.length > MAX_ITEMS}
                  <span class="completion-count">{globalIdx + 1}/{filtered.length}</span>
                {/if}
                <span class="item-source" data-source={item.source}>{item.source}</span>
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
            <span class="ghost-typed">{extraArgs}</span><span class="ghost-text">{extraArgs === "" && lastArgsGhost ? lastArgsGhost : ghostSuffix()}</span>
          </div>
          <input
            type="text"
            class="args-input"
            placeholder={extraArgs || lastArgsGhost || allCompletions.length > 0 ? "" : "extra args..."}
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

  .search-wrap {
    position: relative;
    width: 100%;
  }

  .search-ghost {
    padding: 16px 20px;
    font-size: 18px;
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

  .item-name {
    font-size: 14px;
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

  .item-name.scrolling::-webkit-scrollbar {
    display: none;
  }

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

  .slash-name {
    color: #cba6f7;
    font-family: monospace;
    font-size: 14px;
  }

  :global(.item-source[data-source="Url"]) {
    color: #89b4fa;
  }

  :global(.item-source[data-source="Path"]) {
    color: #a6e3a1;
  }

  :global(.item-source[data-source="History"]) {
    color: #f38ba8;
  }


</style>
