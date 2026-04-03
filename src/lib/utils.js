/**
 * Pure utility functions extracted from +page.svelte for testability.
 * These functions have no Tauri or Svelte dependencies.
 */

/**
 * Returns the index of the first space, slash, or backslash in s, or -1 if none exists.
 * Used by acceptWord() to advance one segment at a time.
 */
export function firstSepIdx(s) {
  return s.search(/[ /\\]/);
}

/**
 * Returns true if the query looks like a filesystem path.
 * Matches: ~, ~/..., ~\..., /..., C:/..., C:\..., \\server\share (UNC)
 */
export function isPathQuery(q) {
  return (
    q === "~" ||
    q.startsWith("~/") ||
    q.startsWith("~\\") ||
    q.startsWith("/") ||
    q.startsWith("\\\\") ||
    /^[a-zA-Z]:[/\\]/.test(q) ||
    /^%[^%]+%/.test(q) ||
    /^\$\{[^}]+\}/.test(q) ||
    /^\$[A-Za-z_]/.test(q) ||
    /^shell:/i.test(q)
  );
}

/**
 * Returns true if all characters of query appear in target in order (fuzzy match).
 * Case-insensitive. Empty query always matches.
 */
export function fuzzyMatch(query, target) {
  if (!query) return true;
  const q = query.toLowerCase();
  const t = target.toLowerCase();
  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) qi++;
  }
  return qi === q.length;
}

/**
 * Returns true if the completion candidate should bypass the Tera template.
 * This happens when:
 *   1. The candidate comes from historyArgs (pre-rendered path), AND
 *   2. The argItem has at least one template arg (contains "{{")
 * In this case the candidate is already fully rendered, so we should
 * launch with args=[] to avoid double-rendering through the template.
 */
export function shouldBypassTemplate(candidate, historyArgs, argItem) {
  const isHistoryEntry = historyArgs.includes(candidate);
  const hasTemplate = argItem?.args?.some((a) => a.includes("{{")) ?? false;
  return isHistoryEntry && hasTemplate;
}

/**
 * Returns the effective search mode for args mode.
 * Priority: session override (argsModeSearchOverride) > per-app completion_search_mode > global uiSearchMode
 *
 * @param {string} mode - current UI mode ("search" | "args")
 * @param {string|null} argsModeSearchOverride - session-level override set by Ctrl+Shift+m in args mode
 * @param {string|null} completionSearchMode - per-app completion_search_mode from config
 * @param {string} uiSearchMode - global search mode
 * @returns {string} effective search mode
 */
export function getEffectiveSearchMode(mode, argsModeSearchOverride, completionSearchMode, uiSearchMode) {
  if (mode === "args") {
    return argsModeSearchOverride ?? completionSearchMode ?? uiSearchMode;
  }
  return uiSearchMode;
}

/**
 * Returns the next search mode after cycling from current.
 *
 * @param {string} current - current search mode
 * @param {string[]} modes - ordered list of modes to cycle through
 * @returns {string} next mode
 */
export function nextSearchMode(current, modes) {
  return modes[(modes.indexOf(current) + 1) % modes.length];
}

/**
 * Returns a synthetic Path item for filesystem path queries.
 */
export function makePathItem(p) {
  return { name: p, path: p, args: [], workdir: null,
           source: "Path", completion: "none", completion_list: [], completion_command: null };
}

/**
 * Returns a synthetic Warning item shown when a config file has parse errors.
 */
export function makeWarningItem(file, error) {
  return { name: file, path: file, args: [], workdir: null, source: "Warning",
    completion: "none", completion_list: [], completion_command: null, _warning_error: error };
}

/**
 * Returns true if the item can accept extra args (i.e. Tab enters args mode).
 * Url, History, and Warning items cannot have args.
 * Path items can have args (e.g. executables discovered via scan_dirs).
 */
export function canHaveArgs(item) {
  return item?.source !== "Url" &&
         item?.source !== "History" && item?.source !== "Warning";
}

const VALID_MODIFIERS = new Set(["Ctrl", "Alt", "Shift", "Meta", "Cmd"]);
const MODIFIER_NAMES = ["Ctrl", "Control", "Alt", "Shift", "Meta", "Cmd", "Super"];

/**
 * Validates a keybindings object and returns an array of [file, message] warning pairs.
 * Detects missing keys, concatenated modifiers (e.g. "Ctrlp"), and unknown modifiers.
 */
export function validateKeybindings(kb) {
  const warnings = [];
  for (const [name, binding] of Object.entries(kb)) {
    if (!binding) continue;
    const parts = binding.split("+");
    const key = parts[parts.length - 1];
    const mods = parts.slice(0, -1);
    if (!key) {
      warnings.push(["config.toml", `keybindings.${name} = "${binding}": missing key`]);
      continue;
    }
    // Detect "Ctrlp" style (modifier name concatenated without "+")
    let detected = false;
    for (const mod of MODIFIER_NAMES) {
      if (key.startsWith(mod) && key.length > mod.length) {
        warnings.push(["config.toml", `keybindings.${name} = "${binding}": did you mean "${[...mods, mod, key.slice(mod.length)].join("+")}"?`]);
        detected = true;
        break;
      }
    }
    if (detected) continue;
    for (const mod of mods) {
      if (!VALID_MODIFIERS.has(mod)) {
        warnings.push(["config.toml", `keybindings.${name} = "${binding}": unknown modifier "${mod}"`]);
        break;
      }
    }
  }
  return warnings;
}

/**
 * Normalizes a raw config file name to the canonical "config.*.toml" form.
 * Examples: "hoge" → "config.hoge.toml", "hoge.toml" → "config.hoge.toml",
 *           "config.hoge.toml" → "config.hoge.toml", "config.toml" → "config.toml"
 */
export function normalizeConfigFileName(raw) {
  let name = raw;
  if (!name.startsWith("config.")) name = "config." + name;
  if (!name.endsWith(".toml")) name = name + ".toml";
  return name;
}

/**
 * Returns true if target matches query under the given completion mode.
 * Modes: "fuzzy" (default), "exact" (substring), "migemo" (romaji→regex via migemoInstance).
 * Falls back to fuzzy when migemoInstance is null or throws.
 *
 * @param {string} query
 * @param {string} target
 * @param {string} completionMode - "fuzzy" | "exact" | "migemo"
 * @param {object|null} migemoInstance - jsmigemo Migemo instance, or null
 */
export function completionMatches(query, target, completionMode, migemoInstance = null) {
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

/**
 * Returns true if the KeyboardEvent matches the binding string.
 * Binding format: "Ctrl+f", "Alt+Space", "Enter", "Ctrl+Shift+P", etc.
 */
export function matchKey(e, binding) {
  const parts = binding.split("+");
  const keyPart = parts[parts.length - 1];
  const ctrl  = parts.includes("Ctrl");
  const alt   = parts.includes("Alt");
  const shift = parts.includes("Shift");
  const meta  = parts.includes("Meta") || parts.includes("Cmd");
  const eventKey = keyPart === "Space" ? " " : keyPart;
  return (
    e.ctrlKey === ctrl &&
    e.altKey === alt &&
    e.shiftKey === shift &&
    e.metaKey === meta &&
    (e.key === eventKey || e.key.toLowerCase() === eventKey.toLowerCase())
  );
}
