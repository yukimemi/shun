<script>
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { debug, info } from "@tauri-apps/plugin-log";
  import { onMount, tick } from "svelte";
  import { firstSepIdx, isPathQuery, matchKey, fuzzyMatch, shouldBypassTemplate } from "$lib/utils.js";

  // migemo インスタンス（lazy load）
  let migemoInstance = $state(null);

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
  let currentPreset = $state("catppuccin-mocha");
  let uiSearchMode = $state("fuzzy");   // "fuzzy" | "exact" | "migemo"
  let uiSortOrder = $state("count_first"); // "count_first" | "recent_first"
  let iconStyle = $state("unicode");    // "unicode" | "svg"

  const THEME_PRESETS = ["catppuccin-mocha", "catppuccin-latte", "nord", "dracula", "tokyo-night"];

  let SLASH_COMMANDS = $derived([
    { name: "/exit",    description: "Quit app" },
    { name: "/config",  description: "Open config file" },
    { name: "/history", description: "Open history file" },
    { name: "/reload",  description: "Reload config (shortcuts, apps, settings)" },
    { name: "/version", description: appVersion ? `v${appVersion}` : "Show version" },
    { name: "/update",  description: updateVersion ? `Update to v${updateVersion}` : "Check for updates" },
    { name: "/theme",   description: `current: ${currentPreset} (Tab to pick)`, completions: THEME_PRESETS },
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

  let _lastSize = { w: 0, h: 0 };

  function _setSize(w, h) {
    if (_lastSize.w === w && _lastSize.h === h) return;
    _lastSize = { w, h };
    win.setSize(new LogicalSize(w, h));
  }

  function resizeForSearch(itemCount) {
    const count = Math.min(itemCount, MAX_ITEMS);
    const h = INPUT_HEIGHT + BORDER_HEIGHT + (count > 0 ? count : 1) * ITEM_HEIGHT + RESULTS_PADDING;
    _setSize(WINDOW_WIDTH, h);
  }

  function resizeForArgs(completionCount) {
    const count = Math.min(completionCount, MAX_COMPLETIONS);
    if (count === 0) {
      _setSize(WINDOW_WIDTH, INPUT_HEIGHT);
    } else {
      const h = INPUT_HEIGHT + BORDER_HEIGHT + count * ITEM_HEIGHT + RESULTS_PADDING;
      _setSize(WINDOW_WIDTH, h);
    }
  }

  function resetToSearch({ skipFocus = false } = {}) {
    mode = "search";
    argItem = null;
    extraArgs = "";
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

  // --- テーマ定義 ---
  const THEMES = {
    "catppuccin-mocha": {
      "--color-bg":      "#1e1e2e",
      "--color-surface": "#313244",
      "--color-overlay": "#45475a",
      "--color-muted":   "#585b70",
      "--color-text":    "#cdd6f4",
      "--color-blue":    "#89b4fa",
      "--color-purple":  "#cba6f7",
      "--color-green":   "#a6e3a1",
      "--color-red":     "#f38ba8",
    },
    "catppuccin-latte": {
      "--color-bg":      "#eff1f5",
      "--color-surface": "#ccd0da",
      "--color-overlay": "#acb0be",
      "--color-muted":   "#9ca0b0",
      "--color-text":    "#4c4f69",
      "--color-blue":    "#1e66f5",
      "--color-purple":  "#8839ef",
      "--color-green":   "#40a02b",
      "--color-red":     "#d20f39",
    },
    "nord": {
      "--color-bg":      "#2e3440",
      "--color-surface": "#3b4252",
      "--color-overlay": "#434c5e",
      "--color-muted":   "#4c566a",
      "--color-text":    "#d8dee9",
      "--color-blue":    "#88c0d0",
      "--color-purple":  "#b48ead",
      "--color-green":   "#a3be8c",
      "--color-red":     "#bf616a",
    },
    "dracula": {
      "--color-bg":      "#282a36",
      "--color-surface": "#44475a",
      "--color-overlay": "#6272a4",
      "--color-muted":   "#6272a4",
      "--color-text":    "#f8f8f2",
      "--color-blue":    "#8be9fd",
      "--color-purple":  "#bd93f9",
      "--color-green":   "#50fa7b",
      "--color-red":     "#ff5555",
    },
    "tokyo-night": {
      "--color-bg":      "#1a1b26",
      "--color-surface": "#24283b",
      "--color-overlay": "#414868",
      "--color-muted":   "#565f89",
      "--color-text":    "#c0caf5",
      "--color-blue":    "#7aa2f7",
      "--color-purple":  "#bb9af7",
      "--color-green":   "#9ece6a",
      "--color-red":     "#f7768e",
    },
  };

  function applyTheme(themeConfig) {
    const preset = themeConfig?.preset || "catppuccin-mocha";
    currentPreset = preset;
    const base = THEMES[preset] ?? THEMES["catppuccin-mocha"];
    const root = document.documentElement;
    // preset のデフォルト色を適用
    for (const [key, val] of Object.entries(base)) {
      root.style.setProperty(key, val);
    }
    // 個別上書き
    const MAP = {
      bg: "--color-bg", surface: "--color-surface", overlay: "--color-overlay",
      muted: "--color-muted", text: "--color-text", blue: "--color-blue",
      purple: "--color-purple", green: "--color-green", red: "--color-red",
    };
    for (const [field, cssVar] of Object.entries(MAP)) {
      if (themeConfig?.[field]) root.style.setProperty(cssVar, themeConfig[field]);
    }
  }

  async function applyConfig({ resetModes = false } = {}) {
    const cfg = await invoke("get_config");
    if (cfg?.keybindings) keybindings = { ...keybindings, ...cfg.keybindings };
    if (cfg?.window_width)    WINDOW_WIDTH    = cfg.window_width;
    if (cfg?.max_items)       MAX_ITEMS       = cfg.max_items;
    if (cfg?.max_completions) MAX_COMPLETIONS = cfg.max_completions;
    if (cfg?.font_size)       document.documentElement.style.setProperty('--font-size', cfg.font_size + 'px');
    if (cfg?.opacity != null) document.documentElement.style.setProperty('--opacity', cfg.opacity);
    if (cfg?.icon_style)      iconStyle     = cfg.icon_style;
    if (resetModes) {
      if (cfg?.search_mode) uiSearchMode = cfg.search_mode;
      if (cfg?.sort_order)  uiSortOrder  = cfg.sort_order;
      applyTheme(cfg?.theme);
    }
  }

  const SEARCH_MODES = ["fuzzy", "exact", "migemo"];
  const SORT_ORDERS = ["count_first", "recent_first"];

  function cycleSearchMode() {
    uiSearchMode = SEARCH_MODES[(SEARCH_MODES.indexOf(uiSearchMode) + 1) % SEARCH_MODES.length];
  }

  function cycleSortOrder() {
    uiSortOrder = SORT_ORDERS[(SORT_ORDERS.indexOf(uiSortOrder) + 1) % SORT_ORDERS.length];
  }

  onMount(async () => {
    await applyConfig({ resetModes: true });
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

  async function onKeydown(e) {
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
        // /theme の args mode: 選択したプリセットを適用して閉じる
        if (argItem?.source === "SlashCmd" && argItem?.name === "/theme") {
          const preset = allCompletions.length > 0
            ? allCompletions[completionIndex]
            : extraArgs.trim();
          if (preset) {
            info(`/theme: applying preset=${preset}`);
            applyTheme({ preset });            // CSS 即時適用（同期）
            resetToSearch({ skipFocus: true }); // focus タイマー不要（隠す直前）
            query = "";                         // query もリセット（/theme が残ると $effect が resize IPC を余分に飛ばす）
            await tick();
            win.hide();
          }
          return;
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
        if (argItem) launchItem(argItem, extraArgs);
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
        if (candidate !== undefined && historyArgs.includes(candidate)) {
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
      // スラッシュコマンドで completions を持つもの（/theme 等）→ args mode でリスト補完
      if (filteredSlash.length > 0) {
        const cmd = filteredSlash[selectedIndex] ?? filteredSlash[0];
        if (cmd.completions?.length > 0) {
          argItem = { name: cmd.name, path: "", args: [], workdir: null,
                      source: "SlashCmd", completion: "list",
                      completion_list: cmd.completions, completion_command: null };
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
          argItem = baseConfigItem
            ? { ...baseConfigItem, history_key: null }
            : { ...item, name: baseName, args: [], source: "ScanDir", history_key: null };
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
          argItem = item;
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
      if (item?.source === "History") {
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
      const count = filteredSlash.length > 0 ? filteredSlash.length : filtered.length;
      resizeForSearch(count);
    } else {
      resizeForArgs(allCompletions.length);
    }
  });

  // search モード: クエリで絞り込み
  $effect(() => {
    if (mode !== "search") return;
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
    invoke("search_items", { query, searchMode: uiSearchMode, sortOrder: uiSortOrder }).then((results) => {
      filtered = results;
      selectedIndex = 0;
      resizeForSearch(results.length);
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
    const completionMode = argItem?.completion_search_mode ?? null;

    // historyArgs を入力でフィルタ（completion_search_mode に応じて切り替え）
    function completionMatches(query, target) {
      if (!query) return true;
      if (completionMode === "migemo" && migemoInstance) {
        try {
          return new RegExp(migemoInstance.query(query), "i").test(target);
        } catch {
          return fuzzyMatch(query, target);
        }
      }
      if (completionMode === "exact") return target.toLowerCase().includes(query.toLowerCase());
      return fuzzyMatch(query, target);
    }
    const filteredHistory = historyArgs.filter((h) => completionMatches(input, h));

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
      itemArgs: argItem?.args ?? null,
      completionSearchMode: argItem?.completion_search_mode ?? null,
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
      // history と重複するものを除外して後ろに追加
      const deduped = pathFull.filter((p) => !filteredHistory.includes(p));
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
    if (cmd.name === "/version") {
      query = `/version — v${appVersion}`;
      setTimeout(() => { query = ""; }, 2000);
      return;
    }
    if (cmd.name === "/theme") {
      query = `/theme — current: ${currentPreset}`;
      setTimeout(() => { query = ""; }, 2000);
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
    resetToSearch();
    await tick();
    win.hide();
    if (cmd.name === "/exit") {
      await invoke("exit_app");
    } else if (cmd.name === "/config") {
      await invoke("open_config");
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
        <div class="status-badges" aria-hidden="true">
          <button class="badge" title="search mode: {uiSearchMode}" onclick={cycleSearchMode}>
            {#if iconStyle === "svg"}
              {#if uiSearchMode === "fuzzy"}
                <!-- 3 wavy lines = fuzzy/approximate -->
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
                  <path d="M2 4.5 C4 3 6 6 8 4.5 C10 3 12 6 14 4.5"/>
                  <path d="M2 8   C4 6.5 6 9.5 8 8   C10 6.5 12 9.5 14 8"/>
                  <path d="M2 11.5 C4 10 6 13 8 11.5 C10 10 12 13 14 11.5"/>
                </svg>
              {:else if uiSearchMode === "exact"}
                <!-- 1 straight line = exact/literal -->
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
                  <line x1="2" y1="8" x2="14" y2="8"/>
                </svg>
              {:else}
                <!-- あ = migemo -->
                <svg width="14" height="14" viewBox="0 0 16 16">
                  <text x="8" y="13" text-anchor="middle" font-size="13" fill="currentColor" font-family="sans-serif">あ</text>
                </svg>
              {/if}
            {:else}
              {uiSearchMode === "fuzzy" ? "≋" : uiSearchMode === "exact" ? "―" : "あ"}
            {/if}
          </button>
          <div class="badge-sep"></div>
          <button class="badge" title="sort order: {uiSortOrder}" onclick={cycleSortOrder}>
            {#if iconStyle === "svg"}
              {#if uiSortOrder === "count_first"}
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
              {uiSortOrder === "count_first" ? "#" : "⌚"}
            {/if}
          </button>
        </div>
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
        <div class="status-badges" aria-hidden="true">
          <button class="badge" title="search mode: {uiSearchMode}" onclick={cycleSearchMode}>
            {#if iconStyle === "svg"}
              {#if uiSearchMode === "fuzzy"}
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
                  <path d="M2 4.5 C4 3 6 6 8 4.5 C10 3 12 6 14 4.5"/>
                  <path d="M2 8   C4 6.5 6 9.5 8 8   C10 6.5 12 9.5 14 8"/>
                  <path d="M2 11.5 C4 10 6 13 8 11.5 C10 10 12 13 14 11.5"/>
                </svg>
              {:else if uiSearchMode === "exact"}
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
                  <line x1="2" y1="8" x2="14" y2="8"/>
                </svg>
              {:else}
                <svg width="14" height="14" viewBox="0 0 16 16">
                  <text x="8" y="13" text-anchor="middle" font-size="13" fill="currentColor" font-family="sans-serif">あ</text>
                </svg>
              {/if}
            {:else}
              {uiSearchMode === "fuzzy" ? "≋" : uiSearchMode === "exact" ? "―" : "あ"}
            {/if}
          </button>
          <div class="badge-sep"></div>
          <button class="badge" title="sort order: {uiSortOrder}" onclick={cycleSortOrder}>
            {#if iconStyle === "svg"}
              {#if uiSortOrder === "count_first"}
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
              {uiSortOrder === "count_first" ? "#" : "⌚"}
            {/if}
          </button>
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
    background: var(--color-bg, #1e1e2e);
    overflow: hidden;
    opacity: var(--opacity, 1);
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

  /* 共通リスト */
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

  .item-name.scrolling::-webkit-scrollbar {
    display: none;
  }

  .completion-path {
    font-size: calc(var(--font-size, 14px) - 1px);
    color: var(--color-text, #cdd6f4);
    font-family: monospace;
  }

  .completion-path.is-dir {
    color: var(--color-blue, #89b4fa);
  }

  .completion-count {
    font-size: calc(var(--font-size, 14px) - 4px);
    color: var(--color-overlay, #45475a);
    flex-shrink: 0;
  }

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

  :global(.item-source[data-source="Url"]) {
    color: var(--color-blue, #89b4fa);
  }

  :global(.item-source[data-source="Path"]) {
    color: var(--color-green, #a6e3a1);
  }

  :global(.item-source[data-source="History"]) {
    color: var(--color-red, #f38ba8);
  }


</style>
