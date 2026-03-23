/**
 * Pure utility functions extracted from +page.svelte for testability.
 * These functions have no Tauri or Svelte dependencies.
 */

/**
 * Returns the index of the first space or slash in s, or -1 if neither exists.
 * Used by acceptWord() to advance one segment at a time.
 */
export function firstSepIdx(s) {
  const spaceIdx = s.indexOf(" ");
  const slashIdx = s.indexOf("/");
  const candidates = [spaceIdx, slashIdx].filter((i) => i !== -1);
  return candidates.length === 0 ? -1 : Math.min(...candidates);
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
    /^[a-zA-Z]:[/\\]/.test(q)
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
