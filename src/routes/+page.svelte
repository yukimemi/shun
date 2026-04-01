<script>
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { debug, info } from "@tauri-apps/plugin-log";
  import { onMount, onDestroy, tick } from "svelte";
  import { firstSepIdx, isPathQuery, matchKey, fuzzyMatch, shouldBypassTemplate, getEffectiveSearchMode, nextSearchMode, makePathItem, makeWarningItem, canHaveArgs, validateKeybindings, normalizeConfigFileName, completionMatches } from "$lib/utils.js";
  import { highlight, shikiTheme } from "$lib/highlight.js";
  import SearchModeIcon from "$lib/SearchModeIcon.svelte";
  import SortOrderIcon from "$lib/SortOrderIcon.svelte";
  import PreviewPanel from "$lib/PreviewPanel.svelte";
  import HelpPanel from "$lib/HelpPanel.svelte";
  import SearchResults from "$lib/SearchResults.svelte";
  import CompletionList from "$lib/CompletionList.svelte";
  import { THEME_PRESETS, applyThemeCssVars } from "$lib/themes.js";

  // migemo インスタンス（lazy load）
  let migemoInstance = $state(null);

  let WINDOW_WIDTH = $state(620);
  let currentWidth = WINDOW_WIDTH; // ユーザーが手動リサイズした幅を追跡（$state 不可: reactive loop になる）
  let PREVIEW_WIDTH = $state(400);
  let MAX_PREVIEW_LINES = $state(500);
  const DRAG_HANDLE_HEIGHT = 16;
  const INPUT_HEIGHT = 52 + DRAG_HANDLE_HEIGHT; // search input + drag handle
  const ITEM_HEIGHT = 40;
  const BORDER_HEIGHT = 1;
  const RESULTS_PADDING = 8;
  let MAX_ITEMS = $state(8);
  let MAX_COMPLETIONS = $state(6);

  const win = getCurrentWindow();

  // keybindings (config から取得、デフォルトはハードコード値) ※matchKey は $lib/utils.js
  let keybindings = $state({
    launch:            "Ctrl+Space",
    next:              "Ctrl+n",
    prev:              "Ctrl+p",
    confirm:           "Enter",
    arg_mode:          "Tab",
    accept_word:       "Ctrl+f",
    accept_line:       "Ctrl+e",
    delete_word:       "Ctrl+w",
    delete_line:       "Ctrl+u",
    run_query:         "Shift+Enter",
    close:             "Escape",
    delete_item:       "Ctrl+d",
    cycle_search_mode:    "Ctrl+Shift+m",
    cycle_sort_order:     "Ctrl+Shift+o",
    toggle_preview:       "Ctrl+Shift+p",
    preview_scroll_down:  "Ctrl+j",
    preview_scroll_up:    "Ctrl+k",
  });


  let appVersion = $state("");
  let updateVersion = $state("");
  let configWarnings = $state([]);
  let currentPreset = $state("catppuccin-mocha");
  let uiSearchMode = $state("fuzzy");   // "fuzzy" | "exact" | "migemo" | "fuzzy_migemo" | "exact_migemo"
  let argsModeSearchOverrides = $state(new Map()); // per-item override: item.name → search mode
  let uiSortOrder = $state("count_first"); // "count_first" | "recent_first"
  let iconStyle = $state("unicode");    // "unicode" | "svg"

  // THEME_PRESETS は themes.js から import
  let configFiles = $state(["config.toml"]);
  let SLASH_COMMANDS = $derived([
    { name: "/exit",    description: "Quit app" },
    { name: "/config",  description: "Open config.toml (Tab to pick other files)", completions: configFiles },
    { name: "/history", description: "Open history file" },
    { name: "/reload",  description: "Reload config (shortcuts, apps, settings)" },
    { name: "/version", description: appVersion ? `v${appVersion}` : "Show version" },
    { name: "/update",  description: updateVersion ? `Update to v${updateVersion}` : "Check for updates" },
    { name: "/theme",   description: `current: ${currentPreset} (Tab to pick)`, completions: THEME_PRESETS },
    { name: "/save",    description: "Save setting to config.local.toml (Tab to pick)", completions: ["monitor", "position", "size", "theme", "search_mode", "sort_order"] },
    { name: "/reset",   description: "Reset setting in config.local.toml (Tab to pick)", completions: ["monitor", "position", "size", "theme", "search_mode", "sort_order"] },
    { name: "/help",    description: "Show keybindings & current status" },
  ]);

  // ヘルプパネル表示フラグ
  let helpVisible = $state(false);
  // スラッシュコマンド結果表示中フラグ（search effect をスキップするため）
  let slashResult = $state(false);
  // プレビュー on/off (モードごと独立)
  let previewArgs = $state(true);
  let previewSearch = $state(false);
  let previewPanelEl = $state(null);
  let previewContent = $state("");
  let previewHighlighted = $state(null);
  // 個々のアイテムのプレビュー対象パス
  let previewTarget = $derived(() => {
    if (mode === "args" && previewArgs && argItem?.completion === "path") {
      const comp = allCompletions[completionIndex];
      return (comp && !comp.endsWith("/")) ? comp : "";
    }
    if (mode === "search" && previewSearch) {
      const item = filtered[selectedIndex];
      const p = item?.path ?? "";
      const skip = !p || p.endsWith("/") || p.endsWith("\\") || item?.source === "Url" || item?.source === "History";
      return skip ? "" : p;
    }
    return "";
  });
  // コンテンツが取得できた場合のみパネル表示（バイナリは表示しない）
  let previewVisible = $derived(previewTarget() !== "" && previewContent !== "");

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

  // args モードでの有効な検索モード:
  //   argsModeSearchOverrides[item.name] (Ctrl+Shift+m で上書き) > completion_search_mode > uiSearchMode
  let effectiveSearchMode = $derived(
    getEffectiveSearchMode(mode, argItem?.name ? (argsModeSearchOverrides.get(argItem.name) ?? null) : null, argItem?.completion_search_mode ?? null, uiSearchMode)
  );

  // search モード: 選択中候補の path がクエリのプレフィックスならghost表示
  let searchGhostSuffix = $derived(() => {
    if (!query || filtered.length === 0) return "";
    const candidate = filtered[selectedIndex]?.path ?? filtered[0]?.path ?? "";
    if (candidate.toLowerCase().startsWith(query.toLowerCase())) {
      return candidate.slice(query.length);
    }
    return "";
  });

  let _lastSize = { w: 0, h: 0 };


  function _setSize(w, h) {
    const totalW = previewVisible ? w + PREVIEW_WIDTH : w;
    // プレビュー表示中は高さを max に固定（候補数に関わらずパネルの高さを一定に保つ）
    const maxH = INPUT_HEIGHT + BORDER_HEIGHT + MAX_ITEMS * ITEM_HEIGHT + RESULTS_PADDING;
    const totalH = previewVisible ? maxH : h;
    if (_lastSize.w === totalW && _lastSize.h === totalH) return;
    _lastSize = { w: totalW, h: totalH };
    currentWidth = w; // プレビュー幅を含まないランチャー幅を記録
    win.setSize(new LogicalSize(totalW, totalH));
  }

  function resizeForSearch(itemCount) {
    const count = Math.min(itemCount, MAX_ITEMS);
    const h = INPUT_HEIGHT + BORDER_HEIGHT + (count > 0 ? count : 1) * ITEM_HEIGHT + RESULTS_PADDING;
    _setSize(currentWidth, h);
  }

  function resizeForArgs(completionCount) {
    const count = Math.min(completionCount, MAX_COMPLETIONS);
    if (count === 0) {
      _setSize(currentWidth, INPUT_HEIGHT);
    } else {
      const h = INPUT_HEIGHT + BORDER_HEIGHT + count * ITEM_HEIGHT + RESULTS_PADDING;
      _setSize(currentWidth, h);
    }
  }

  function applyArgItem(newItem) {
    argItem = newItem;
  }

  function resetToSearch({ skipFocus = false, keepQuery = false } = {}) {
    mode = "search";
    helpVisible = false;
    argItem = null;
    extraArgs = "";
    if (!keepQuery) {
      query = "";
      filtered = [];
    }
    completionPrefix = "";
    allCompletions = [];
    completionIndex = 0;
    lastArgsGhost = "";
    historyArgs = [];
    resizeForSearch(filtered.length);
    if (!skipFocus) setTimeout(() => inputEl?.focus(), 10);
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
    const el = mode === "args" ? argsEl : inputEl;
    if (!el) return;
    el.focus();
    const val = mode === "args" ? extraArgs : query;
    const pos = el.selectionStart ?? val.length;
    let i = pos - 1;
    while (i >= 0 && (val[i] === " " || val[i] === "/")) i--;
    while (i >= 0 && val[i] !== " " && val[i] !== "/") i--;
    const newVal = val.slice(0, i + 1) + val.slice(pos);
    if (mode === "args") extraArgs = newVal; else query = newVal;
    setTimeout(() => { el.selectionStart = el.selectionEnd = i + 1; }, 0);
  }

  function deleteLine() {
    if (mode === "args") {
      extraArgs = "";
    } else {
      query = "";
    }
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
    extraArgs = candidate;
    allCompletions = [];
  }

  function applyTheme(themeConfig) {
    currentPreset = applyThemeCssVars(themeConfig);
  }

  async function applyConfig({ resetModes = false } = {}) {
    const { config: cfg, warnings: backendWarnings } = await invoke("get_config_and_warnings");
    configFiles = await invoke("list_config_files");
    if (cfg?.keybindings) keybindings = { ...keybindings, ...cfg.keybindings };
    if (cfg?.window_width)      { WINDOW_WIDTH = cfg.window_width; currentWidth = WINDOW_WIDTH; }
    if (cfg?.max_items)         MAX_ITEMS         = cfg.max_items;
    if (cfg?.max_completions)   MAX_COMPLETIONS   = cfg.max_completions;
    if (cfg?.preview_width)          PREVIEW_WIDTH  = cfg.preview_width;
    if (cfg?.max_preview_lines)      MAX_PREVIEW_LINES = cfg.max_preview_lines;
    if (cfg?.preview_args  != null)  previewArgs    = cfg.preview_args;
    if (cfg?.preview_search != null) previewSearch  = cfg.preview_search;
    document.documentElement.style.setProperty('--launcher-width', (cfg?.window_width ?? WINDOW_WIDTH) + 'px');
    if (cfg?.font_size)       document.documentElement.style.setProperty('--font-size', cfg.font_size + 'px');
    if (cfg?.opacity != null) document.documentElement.style.setProperty('--opacity', cfg.opacity);
    if (cfg?.icon_style)      iconStyle     = cfg.icon_style;
    if (resetModes) {
      if (cfg?.search_mode) uiSearchMode = cfg.search_mode;
      if (cfg?.sort_order)  uiSortOrder  = cfg.sort_order;
      applyTheme(cfg?.theme);
    }
    const kbWarnings = cfg?.keybindings ? validateKeybindings(cfg.keybindings) : [];
    configWarnings = [...backendWarnings, ...kbWarnings];
  }

  const SEARCH_MODES = ["fuzzy", "exact", "migemo", "fuzzy_migemo", "exact_migemo"];
  const SORT_ORDERS = ["count_first", "recent_first"];

  function cycleSearchMode() {
    if (mode === "args" && argItem?.name) {
      argsModeSearchOverrides = new Map(argsModeSearchOverrides).set(argItem.name, nextSearchMode(effectiveSearchMode, SEARCH_MODES));
    } else {
      uiSearchMode = nextSearchMode(uiSearchMode, SEARCH_MODES);
    }
  }

  function cycleSortOrder() {
    uiSortOrder = SORT_ORDERS[(SORT_ORDERS.indexOf(uiSortOrder) + 1) % SORT_ORDERS.length];
  }

  // ユーザーの手動リサイズを追跡（プレビュー幅は除く）
  // $state/$effect 不可: currentWidth を $state にすると resizeForSearch 内で読まれ reactive loop になる
  const _handleResize = () => {
    currentWidth = previewVisible ? window.innerWidth - PREVIEW_WIDTH : window.innerWidth;
  };
  onDestroy(() => window.removeEventListener("resize", _handleResize));

  onMount(async () => {
    await applyConfig({ resetModes: true });
    appVersion = await getVersion();

    window.addEventListener("resize", _handleResize);

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

    // migemo 辞書を background でロード（失敗しても他の search mode にフォールバック）
    import("jsmigemo").then(async ({ Migemo, CompactDictionary }) => {
      try {
        const res = await fetch("/migemo-compact-dict.bin");
        const buf = await res.arrayBuffer();
        const dict = new CompactDictionary(buf);
        const m = new Migemo();
        m.setDict(dict);
        migemoInstance = m;
      } catch (e) {
        console.warn("migemo load failed:", e);
      }
    });

    await listen("show-launcher", async () => {
      debug("show-launcher: resetting state");
      mode = "search";
      argItem = null;
      extraArgs = "";
      query = "";
      completionPrefix = "";
      allCompletions = [];
      completionIndex = 0;
      lastArgsGhost = "";
      historyArgs = [];
      // 設定を再読み込み（keybindings, font_size, theme 等を config.toml 変更後に即反映）
      await applyConfig();
      // 表示時に必ずサイズを正しく戻す（WebView2 の描画キャッシュ対策）
      resizeForSearch(filtered.length || MAX_ITEMS);
      setTimeout(() => inputEl?.focus(), 30);
    });
  });

  // スラッシュコマンドの args mode で Enter/Shift+Enter が押されたときの処理。
  // 対象コマンドなら処理して true を返す。通常アイテムは false を返す。
  async function confirmSlashArg(cmdName, value) {
    if (!value) return false;
    if (cmdName === "/config") {
      const name = normalizeConfigFileName(value);
      resetToSearch({ skipFocus: true });
      await tick();
      win.hide();
      await invoke("open_config", { name });
      return true;
    }
    if (cmdName === "/theme") {
      info(`/theme: applying preset=${value}`);
      applyTheme({ preset: value }); // CSS 即時適用（同期）
      resetToSearch({ skipFocus: true });
      await tick();
      win.hide();
      return true;
    }
    if (cmdName === "/save") {
      const valueMap = {
        theme:       currentPreset,
        search_mode: uiSearchMode,
        sort_order:  uiSortOrder,
        monitor:     "", // Rust 側で自動検出
        position:    "", // Rust 側で自動検出
        size:        "", // Rust 側で自動検出
      };
      try {
        const msg = await invoke("save_to_local", { key: value, value: valueMap[value] ?? "" });
        query = `/save — ${msg}`;
        if (value === "size") {
          // window_width が変わったので即反映
          await applyConfig();
          resizeForSearch(filtered.length);
        }
      } catch (e) {
        query = `/save — error: ${e}`;
      }
      resetToSearch({ skipFocus: true });
      await tick();
      win.hide();
      return true;
    }
    if (cmdName === "/reset") {
      try {
        const msg = await invoke("reset_local", { key: value });
        query = `/reset — ${msg}`;
        if (value === "size") {
          // デフォルト幅に戻す (applyConfig が currentWidth = WINDOW_WIDTH を更新)
          await applyConfig();
          _lastSize = { w: 0, h: 0 }; // キャッシュクリアして強制リサイズ
          resizeForSearch(filtered.length);
        }
      } catch (e) {
        query = `/reset — error: ${e}`;
      }
      resetToSearch({ skipFocus: true });
      await tick();
      win.hide();
      return true;
    }
    return false;
  }

  async function handleArgsKeydown(e) {
    if (matchKey(e, keybindings.close)) {
      e.preventDefault();
      if (allCompletions.length > 0) {
        allCompletions = [];
      } else {
        resetToSearch({ keepQuery: true });
      }
    } else if (matchKey(e, keybindings.confirm)) {
      e.preventDefault();
      if (argItem?.source === "SlashCmd") {
        const value = allCompletions.length > 0 ? allCompletions[completionIndex] : extraArgs.trim();
        if (await confirmSlashArg(argItem.name, value)) return;
      }
      if (allCompletions.length > 0) {
        const candidate = allCompletions[completionIndex];
        applySelectedCompletion();
        if (!candidate.endsWith('/') && argItem) {
          // history エントリ + テンプレート args の場合: レンダリング済みパスをそのまま渡す
          if (shouldBypassTemplate(candidate, historyArgs, argItem)) {
            launchItem({ ...argItem, args: [] }, candidate);
          } else {
            launchItem(argItem, extraArgs);
          }
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
    } else if (matchKey(e, keybindings.run_query)) {
      // 補完を無視して入力をそのまま起動
      e.preventDefault();
      if (argItem?.source === "SlashCmd") {
        await confirmSlashArg(argItem.name, extraArgs.trim());
      } else if (argItem) {
        launchItem(argItem, extraArgs);
      }
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
    } else if (matchKey(e, keybindings.delete_item)) {
      e.preventDefault();
      const candidate = allCompletions[completionIndex];
      // /config: config.toml 以外のファイルを削除
      if (argItem?.source === "SlashCmd" && argItem?.name === "/config") {
        if (candidate && candidate !== "config.toml") {
          await invoke("delete_config_file", { name: candidate });
          configFiles = configFiles.filter((f) => f !== candidate);
          allCompletions = allCompletions.filter((_, i) => i !== completionIndex);
          completionIndex = Math.min(completionIndex, allCompletions.length - 1);
        }
      } else if (candidate !== undefined && historyArgs.includes(candidate)) {
        const baseKey = argItem?.source === "Config" ? argItem.name : argItem?.path ?? "";
        invoke("delete_history_item", { key: `${baseKey}\t${candidate}` });
        historyArgs = historyArgs.filter((a) => a !== candidate);
        allCompletions = allCompletions.filter((_, i) => i !== completionIndex);
        completionIndex = Math.min(completionIndex, allCompletions.length - 1);
      }
    } else if (matchKey(e, keybindings.cycle_search_mode)) {
      e.preventDefault();
      cycleSearchMode();
    } else if (matchKey(e, keybindings.cycle_sort_order)) {
      e.preventDefault();
      cycleSortOrder();
    } else if (matchKey(e, keybindings.toggle_preview)) {
      e.preventDefault();
      previewArgs = !previewArgs;
    } else if (matchKey(e, keybindings.preview_scroll_down)) {
      if (previewPanelEl) { e.preventDefault(); previewPanelEl.scrollBy(0, 60); }
    } else if (matchKey(e, keybindings.preview_scroll_up)) {
      if (previewPanelEl) { e.preventDefault(); previewPanelEl.scrollBy(0, -60); }
    }
  }

  async function handleSearchKeydown(e) {
    if (matchKey(e, keybindings.close)) {
      e.preventDefault();
      resetToSearch({ skipFocus: true });
      await tick();
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
      // スラッシュコマンドで completions を持つもの（/theme 等）→ args mode でリスト補完
      if (filteredSlash.length > 0) {
        const cmd = filteredSlash[selectedIndex] ?? filteredSlash[0];
        if (cmd.completions?.length > 0) {
          applyArgItem({ name: cmd.name, path: "", args: [], workdir: null,
                         source: "SlashCmd", completion: "list",
                         completion_list: cmd.completions, completion_command: null });
          extraArgs = "";
          allCompletions = cmd.completions;
          completionIndex = 0;
          lastArgsGhost = cmd.completions[0] ?? "";
          historyArgs = [];
          mode = "args";
          win.setSize(new LogicalSize(WINDOW_WIDTH, INPUT_HEIGHT));
          setTimeout(() => argsEl?.focus(), 10);
        }
        return;
      }
      if (isPathQuery(query) && filtered[selectedIndex]) {
        query = filtered[selectedIndex].path;
      } else {
        const item = filtered[selectedIndex];
        if (item?.source === "History") {
          // History アイテム (path\targs) → base exe でargs modeに入り、既存argsはghost textで提示
          const baseName = item.name.split(" › ")[0];
          // history_key の tab より前がベースキー:
          //   Config アイテム → "MemoNew"  (name)
          //   ScanDir アイテム → "C:/Windows/System32/taskkill.exe"  (full path)
          const baseKey = item.history_key
            ? item.history_key.split("\t")[0]
            : baseName;
          // Config アイテム由来の History の場合、filtered から元の Config アイテムを引き当てて
          // テンプレート args を引き継ぐ（args: [] で上書きするとテンプレートが消えるため）
          // baseKey が name の場合（新形式: "MemoNew\t..."）と
          // path の場合（旧形式: "nvim\t..."）の両方を検索する
          const baseConfigItem = filtered.find(
            (i) => i.source === "Config" && (i.name === baseKey || i.path === baseKey),
          );
          applyArgItem(baseConfigItem
            ? { ...baseConfigItem, history_key: null }
            : { ...item, name: baseName, args: [], source: "ScanDir", history_key: null });
          extraArgs = "";
          mode = "args";
          lastArgsGhost = item.args.join(" ");
          historyArgs = [];
          win.setSize(new LogicalSize(WINDOW_WIDTH, INPUT_HEIGHT));
          setTimeout(() => argsEl?.focus(), 10);
          // Config アイテムが見つかった場合は name をキーに使って正しい history を引く
          // （旧形式 "nvim\t..." の場合 baseKey が path になるため name で引き直す）
          const histKeyForArgs = baseConfigItem ? baseConfigItem.name : baseKey;
          invoke("get_args_history", { path: histKeyForArgs }).then((candidates) => {
            historyArgs = candidates;
            if (candidates.length > 0) lastArgsGhost = candidates[0];
          });
        } else if (canHaveArgs(item)) {
          applyArgItem(item);
          mode = "args";
          lastArgsGhost = "";
          historyArgs = [];
          win.setSize(new LogicalSize(WINDOW_WIDTH, INPUT_HEIGHT));
          setTimeout(() => argsEl?.focus(), 10);
          // Config アイテムは name をキーに記録しているので name で引く
          const histKey = item.source === "Config" ? item.name : item.path;
          invoke("get_args_history", { path: histKey }).then((candidates) => {
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
      } else if (filtered[selectedIndex]?.source === "Warning") {
        invoke("open_config", { name: filtered[selectedIndex].path });
      } else if (filtered[selectedIndex]) {
        const item = isPathQuery(query)
          ? makePathItem(query)
          : filtered[selectedIndex];
        launchItem(item, null);
      }
    } else if (matchKey(e, keybindings.delete_word)) {
      e.preventDefault();
      deleteWord();
    } else if (matchKey(e, keybindings.delete_line)) {
      e.preventDefault();
      deleteLine();
    } else if (matchKey(e, keybindings.delete_item)) {
      e.preventDefault();
      const item = filtered[selectedIndex];
      if (item && ["History", "Url", "Path"].includes(item.source)) {
        invoke("delete_history_item", { key: item.history_key ?? item.path }).then(() => {
          invoke("reload");
        });
        filtered = filtered.filter((_, i) => i !== selectedIndex);
        selectedIndex = Math.min(selectedIndex, filtered.length - 1);
        resizeForSearch(filtered.length);
      }
    } else if (matchKey(e, keybindings.cycle_search_mode)) {
      e.preventDefault();
      cycleSearchMode();
    } else if (matchKey(e, keybindings.cycle_sort_order)) {
      e.preventDefault();
      cycleSortOrder();
    } else if (matchKey(e, keybindings.toggle_preview)) {
      e.preventDefault();
      previewSearch = !previewSearch;
    } else if (matchKey(e, keybindings.preview_scroll_down)) {
      if (previewPanelEl) { e.preventDefault(); previewPanelEl.scrollBy(0, 60); }
    } else if (matchKey(e, keybindings.preview_scroll_up)) {
      if (previewPanelEl) { e.preventDefault(); previewPanelEl.scrollBy(0, -60); }
    }
  }

  async function onKeydown(e) {
    if (helpVisible) {
      e.preventDefault();
      helpVisible = false;
      query = "";
      _setSize(WINDOW_WIDTH, INPUT_HEIGHT);
      return;
    }
    if (mode === "args") {
      return handleArgsKeydown(e);
    }
    return handleSearchKeydown(e);
  }

  // previewContent が確定したタイミングでウィンドウサイズ再調整（位置はそのまま）
  $effect(() => {
    previewTarget(); // reactive dep として登録
    previewContent;  // previewContent を直接読んでトラッキング保証
    _lastSize = { w: 0, h: 0 }; // キャッシュクリアして強制リサイズ
    if (helpVisible) return;     // ヘルプパネル表示中はサイズを変えない
    if (mode === "args") {
      resizeForArgs(allCompletions.length);
    } else {
      // スラッシュコマンドモード時は filteredSlash.length を使う（filtered は [] になるため）
      resizeForSearch(filteredSlash.length > 0 ? filteredSlash.length : filtered.length);
    }
  });

  // プレビューコンテンツ取得 + ハイライト（Ctrl+n/p でアイテムが変わっても位置・サイズは変化しない）
  $effect(() => {
    const target = previewTarget();
    if (!target) { previewContent = ""; previewHighlighted = null; return; }
    invoke("read_preview", { path: target, maxLines: MAX_PREVIEW_LINES }).then(text => {
      previewContent = text;
      previewHighlighted = null; // いったんプレーンテキストを表示
      if (!text) return;
      // 非同期でハイライト（完了したら差し替え）
      shikiTheme(currentPreset).then(theme =>
        highlight(target, text, theme).then(html => {
          if (previewTarget() === target) previewHighlighted = html;
        })
      );
    });
  });

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
    if (helpVisible) return;     // ヘルプパネル表示中はサイズを変えない
    if (slashResult) return;     // スラッシュコマンド結果表示中はサイズを変えない
    if (mode === "search") {
      const count = filteredSlash.length > 0 ? filteredSlash.length : filtered.length;
      resizeForSearch(count);
    } else {
      resizeForArgs(allCompletions.length);
    }
  });

  // search モード: クエリで絞り込み
  $effect(() => {
    if (mode !== "search" || helpVisible) return;
    // スラッシュコマンド結果表示中は検索しない
    if (slashResult) return;
    // スラッシュで始まり、かつ一致するスラッシュコマンドがある場合のみスラッシュコマンドモード
    // （/Applications/... などの Unix パスはスルー）
    if (query.startsWith("/") && filteredSlash.length > 0) {
      filtered = [];
      selectedIndex = 0;
      resizeForSearch(filteredSlash.length);
      return;
    }
    if (query.startsWith("http://") || query.startsWith("https://")) {
      invoke("search_items", { query, searchMode: uiSearchMode, sortOrder: uiSortOrder }).then((results) => {
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
      Promise.all([
        invoke("complete_path", { input: query, completionType: "path", completionList: [], completionCommand: null, workdir: null }),
        invoke("search_items", { query, searchMode: uiSearchMode, sortOrder: uiSortOrder }),
      ]).then(([pathResult, searchResults]) => {
        const historyItems = searchResults.filter(i => i.source === "Path" || i.source === "History");
        const pathCompletions = pathResult.completions.map(makePathItem);
        const historyPathSet = new Set(historyItems.map(i => i.path));
        const uniqueCompletions = pathCompletions.filter(i => !historyPathSet.has(i.path));
        const combined = [...historyItems, ...uniqueCompletions];
        filtered = combined.length > 0 ? combined : [makePathItem(query)];
        selectedIndex = 0;
        resizeForSearch(filtered.length);
      });
      return;
    }
    // Read configWarnings synchronously so $effect tracks it as a dependency
    const currentWarnings = configWarnings;
    invoke("search_items", { query, searchMode: uiSearchMode, sortOrder: uiSortOrder }).then((results) => {
      const warnItems = !query ? currentWarnings.map(([file, error]) => makeWarningItem(file, error)) : [];
      filtered = [...warnItems, ...results];
      selectedIndex = 0;
      resizeForSearch(filtered.length);
    });
  });

  // args モード: extraArgs / historyArgs 変化で補完を更新
  // allCompletions はすべて「extraArgs に直接セットできるフル文字列」で統一
  $effect(() => {
    if (mode !== "args") return;

    // SlashCmd (/theme 等) は completion_list を直接フィルタ（history なし）
    if (argItem?.source === "SlashCmd") {
      const list = argItem?.completion_list ?? [];
      const input = extraArgs.toLowerCase();
      const newCompletions = input ? list.filter((c) => fuzzyMatch(input, c)) : list;
      allCompletions = newCompletions;
      completionIndex = 0;
      resizeForArgs(newCompletions.length);
      return;
    }

    const input = extraArgs;
    const completionMode = effectiveSearchMode;

    // historyArgs を入力でフィルタ（effectiveSearchMode に応じて切り替え）
    const filteredHistory = historyArgs.filter((h) => completionMatches(input, h, completionMode, migemoInstance));

    // list 補完: 未入力でも completion_list を即時表示
    if (!input && argItem?.completion === "list") {
      const list = argItem?.completion_list ?? [];
      const deduped = list.filter((c) => !filteredHistory.includes(c));
      completionPrefix = "";
      const merged = [...filteredHistory, ...deduped];
      allCompletions = merged;
      completionIndex = 0;
      resizeForArgs(merged.length);
      return;
    }

    if (!input && (!argItem?.completion || argItem?.completion === "none")) {
      // 補完なし: history のみ表示
      completionPrefix = "";
      allCompletions = filteredHistory;
      completionIndex = 0;
      resizeForArgs(filteredHistory.length);
      return;
    }

    // 入力あり (または path/command 補完で未入力): path/command 補完と history をマージ
    invoke("complete_path", {
      input,
      completionType: argItem?.completion ?? "path",
      completionList: argItem?.completion_list ?? [],
      completionCommand: argItem?.completion_command ?? null,
      workdir: argItem?.workdir ?? null,
      itemArgs: argItem?.args ?? null,
      completionSearchMode: effectiveSearchMode,
    }).then((result) => {
      // Rust 側でフィルタ済み。prefix 一致を上位に並べる
      const stem = input.slice(result.prefix.length).toLowerCase();
      const sorted = stem
        ? result.completions.slice().sort((a, b) => {
            const aPrefix = a.toLowerCase().startsWith(stem);
            const bPrefix = b.toLowerCase().startsWith(stem);
            if (aPrefix !== bPrefix) return aPrefix ? -1 : 1;
            return a.localeCompare(b);
          })
        : result.completions;
      // path 補完はフル文字列に展開（prefix + item）
      const pathFull = sorted.map((c) => result.prefix + c);
      // history と重複するものを除外（path 補完は base_path strip 済みの相対名にも対応）
      const isPath = argItem?.completion === "path";
      const deduped = pathFull.filter((p) =>
        !filteredHistory.some((h) =>
          h === p || (isPath && (h.endsWith("/" + p) || h.endsWith("\\" + p)))
        )
      );
      completionPrefix = "";
      const newAllCompletions = [...filteredHistory, ...deduped];
      allCompletions = newAllCompletions;
      completionIndex = 0;
      resizeForArgs(newAllCompletions.length);
    });
  });

  // スラッシュコマンドの絞り込み
  let filteredSlash = $derived(
    query.startsWith("/")
      ? SLASH_COMMANDS.filter((c) => c.name.startsWith(query.toLowerCase()))
      : []
  );

  async function runSlashCommand(cmd) {
    if (cmd.name === "/save" || cmd.name === "/reset") {
      // Enter on /save or /reset → enter args mode to pick which setting
      applyArgItem({ name: cmd.name, path: "", args: [], workdir: null,
                     source: "SlashCmd", completion: "list",
                     completion_list: cmd.completions, completion_command: null });
      extraArgs = "";
      allCompletions = cmd.completions;
      completionIndex = 0;
      lastArgsGhost = "";
      historyArgs = [];
      mode = "args";
      await tick();
      argsEl?.focus();
      return;
    }
    if (cmd.name === "/help") {
      query = "/help";
      helpVisible = true;
      // ヘルプパネルの高さ: ステータス3行 + 共通7行 + args 2行 = 12行 + divider×2 + padding + footer
      const HELP_ROW_HEIGHT = 30;
      const HELP_ROWS = 14;
      const HELP_EXTRA = 17 + 8 + 12 + 28; // divider + panel padding + section padding + footer
      _setSize(WINDOW_WIDTH, INPUT_HEIGHT + BORDER_HEIGHT + HELP_ROWS * HELP_ROW_HEIGHT + HELP_EXTRA + RESULTS_PADDING * 2);
      return;
    }
    if (cmd.name === "/version") {
      slashResult = true;
      query = `/version — v${appVersion}`;
      setTimeout(() => { query = ""; slashResult = false; }, 2000);
      return;
    }
    if (cmd.name === "/theme") {
      slashResult = true;
      query = `/theme — current: ${currentPreset}`;
      setTimeout(() => { query = ""; slashResult = false; }, 2000);
      return;
    }
    if (cmd.name === "/update") {
      slashResult = true;
      query = updateVersion ? `/update — starting download...` : `/update — checking...`;
      try {
        await invoke("install_update");
        query = `/update — already up to date`;
        setTimeout(() => { query = ""; slashResult = false; }, 2000);
      } catch (e) {
        query = `/update — error: ${e}`;
        setTimeout(() => { query = ""; slashResult = false; }, 3000);
      }
      return;
    }
    resetToSearch();
    await tick();
    win.hide();
    if (cmd.name === "/exit") {
      await invoke("exit_app");
    } else if (cmd.name === "/config") {
      await invoke("open_config", { name: "config.toml" });
    } else if (cmd.name === "/history") {
      await invoke("open_history");
    } else if (cmd.name === "/reload") {
      await invoke("reload");
      await applyConfig({ resetModes: true });
    }
  }

  async function launchItem(item, args) {
    const extraArgsList = args ? args.trim().split(/\s+/).filter(Boolean) : [];
    try {
      await invoke("launch_item", { item, extraArgs: extraArgsList });
    } catch (e) {
      console.error("launch failed:", e);
    }
    // reset → tick で描画確定 → hide の順にすることで
    // WebView2 が正しいサイズ・状態で描画キャッシュを持ったまま隠れる
    resetToSearch();
    await tick();
    win.hide();
  }

  function focusInput(el) {
    el.focus();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <div class="launcher" class:preview-open={previewVisible}>
    <div class="drag-handle" onmousedown={() => win.startDragging()}>
      <span class="drag-grip">⠿</span>
    </div>
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
        <div class="status-badges" aria-hidden="true">
          <button class="badge" title="search mode: {uiSearchMode}" onclick={cycleSearchMode}>
            <SearchModeIcon mode={uiSearchMode} {iconStyle} />
          </button>
          <div class="badge-sep"></div>
          <button class="badge" title="sort order: {uiSortOrder}" onclick={cycleSortOrder}>
            <SortOrderIcon order={uiSortOrder} {iconStyle} />
          </button>
        </div>
      </div>
      {#if helpVisible}
        <HelpPanel {keybindings} {currentPreset} {uiSearchMode} {uiSortOrder} />
      {:else}
        <SearchResults
          {filtered}
          {filteredSlash}
          bind:selectedIndex
          {slashResult}
          {MAX_ITEMS}
          onrunslash={runSlashCommand}
          onlaunch={(item) => launchItem(item, null)}
          onopenconfig={(name) => invoke("open_config", { name })}
        />
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
        <div class="status-badges" aria-hidden="true">
          <button class="badge" title="search mode: {effectiveSearchMode}" onclick={cycleSearchMode}>
            <SearchModeIcon mode={effectiveSearchMode} {iconStyle} />
          </button>
          <div class="badge-sep"></div>
          <button class="badge" title="sort order: {uiSortOrder}" onclick={cycleSortOrder}>
            <SortOrderIcon order={uiSortOrder} {iconStyle} />
          </button>
        </div>
      </div>
      <CompletionList
        {allCompletions}
        bind:completionIndex
        {MAX_COMPLETIONS}
        {historyArgs}
        onselectcompletion={selectCompletion}
        onapplycompletion={applySelectedCompletion}
      />
    {/if}
  </div>
  {#if previewVisible}
    <PreviewPanel highlighted={previewHighlighted} content={previewContent} bind:el={previewPanelEl} />
  {/if}
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
    display: flex;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: transparent;
  }

  .launcher {
    flex: 1;
    height: 100%;
    background: var(--color-bg, #1e1e2e);
    overflow: hidden;
    opacity: var(--opacity, 1);
  }

  .launcher.preview-open {
    flex: 0 0 var(--launcher-width, 620px);
  }

  .drag-handle {
    height: 16px;
    width: 100%;
    cursor: grab;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .drag-handle:active {
    cursor: grabbing;
  }

  .drag-grip {
    font-size: 12px;
    color: var(--color-surface1, #313244);
    line-height: 1;
    user-select: none;
    pointer-events: none;
  }

  .search-wrap {
    position: relative;
    width: 100%;
  }

  .status-badges {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    pointer-events: auto;
    background: var(--color-surface, #313244);
    border-radius: 6px;
    padding: 2px 2px;
    opacity: 0.7;
    transition: opacity 0.15s;
  }

  .status-badges:hover {
    opacity: 1;
  }

  .args-bar .status-badges {
    position: static;
    transform: none;
    flex-shrink: 0;
  }

  .badge {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    font-size: 12px;
    line-height: 1;
    color: var(--color-text, #cdd6f4);
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    padding: 0;
    transition: background 0.1s;
    font-family: inherit;
  }

  .badge:hover {
    background: var(--color-overlay, #45475a);
  }

  .badge-sep {
    width: 1px;
    height: 14px;
    background: var(--color-overlay, #45475a);
    flex-shrink: 0;
  }

  .search-ghost {
    padding: 16px 60px 16px 20px;
    font-size: calc(var(--font-size, 14px) + 4px);
  }

  .search {
    width: 100%;
    padding: 16px 60px 16px 20px;
    font-size: calc(var(--font-size, 14px) + 4px);
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text, #cdd6f4);
    font-family: inherit;
  }

  .search::placeholder {
    color: var(--color-muted, #585b70);
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
    font-size: calc(var(--font-size, 14px) + 4px);
    color: var(--color-blue, #89b4fa);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .args-sep {
    font-size: calc(var(--font-size, 14px) + 4px);
    color: var(--color-overlay, #45475a);
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
    font-size: calc(var(--font-size, 14px) + 4px);
    font-family: inherit;
    white-space: pre;
    overflow: hidden;
  }

  .ghost-typed { color: transparent; }
  .ghost-text  { color: var(--color-overlay, #45475a); }

  .args-input {
    position: relative;
    z-index: 1;
    width: 100%;
    font-size: calc(var(--font-size, 14px) + 4px);
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text, #cdd6f4);
    font-family: inherit;
  }

  .args-input::placeholder { color: var(--color-muted, #585b70); }


</style>
