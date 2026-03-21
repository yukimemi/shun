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
