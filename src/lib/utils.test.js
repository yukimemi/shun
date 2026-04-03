import { describe, it, expect } from "vitest";
import { firstSepIdx, isPathQuery, matchKey, fuzzyMatch, shouldBypassTemplate, getEffectiveSearchMode, nextSearchMode, makePathItem, makeWarningItem, canHaveArgs, validateKeybindings, normalizeConfigFileName, completionMatches } from "./utils.js";

// --- firstSepIdx ---

describe("firstSepIdx", () => {
  it("returns -1 for empty string", () => {
    expect(firstSepIdx("")).toBe(-1);
  });

  it("returns -1 when no space or slash", () => {
    expect(firstSepIdx("hello")).toBe(-1);
  });

  it("finds space", () => {
    expect(firstSepIdx("foo bar")).toBe(3);
  });

  it("finds slash", () => {
    expect(firstSepIdx("foo/bar")).toBe(3);
  });

  it("returns minimum when both present - space first", () => {
    expect(firstSepIdx("a b/c")).toBe(1);
  });

  it("returns minimum when both present - slash first", () => {
    expect(firstSepIdx("a/b c")).toBe(1);
  });

  it("finds separator at index 0", () => {
    expect(firstSepIdx(" foo")).toBe(0);
    expect(firstSepIdx("/foo")).toBe(0);
  });

  it("finds backslash", () => {
    expect(firstSepIdx("Users\\foo")).toBe(5);
    expect(firstSepIdx("C:\\Users\\foo")).toBe(2);
  });

  it("returns minimum when backslash before slash", () => {
    expect(firstSepIdx("foo\\bar/baz")).toBe(3);
  });

  it("returns minimum when slash before backslash", () => {
    expect(firstSepIdx("foo/bar\\baz")).toBe(3);
  });
});

// --- isPathQuery ---

describe("isPathQuery", () => {
  it("detects tilde alone", () => {
    expect(isPathQuery("~")).toBe(true);
  });

  it("detects tilde with forward slash", () => {
    expect(isPathQuery("~/Documents")).toBe(true);
  });

  it("detects tilde with backslash", () => {
    expect(isPathQuery("~\\AppData")).toBe(true);
  });

  it("detects Unix absolute path", () => {
    expect(isPathQuery("/usr/bin")).toBe(true);
  });

  it("detects Windows drive forward slash", () => {
    expect(isPathQuery("C:/Users")).toBe(true);
    expect(isPathQuery("D:/")).toBe(true);
    expect(isPathQuery("z:/data")).toBe(true);
  });

  it("detects Windows drive backslash", () => {
    expect(isPathQuery("C:\\Users")).toBe(true);
  });

  it("rejects plain query", () => {
    expect(isPathQuery("firefox")).toBe(false);
  });

  it("rejects drive letter without separator", () => {
    expect(isPathQuery("C:")).toBe(false);
  });

  it("rejects relative path", () => {
    expect(isPathQuery("usr/bin/bash")).toBe(false);
  });

  it("rejects https url", () => {
    expect(isPathQuery("https://example.com")).toBe(false);
  });

  it("rejects empty string", () => {
    expect(isPathQuery("")).toBe(false);
  });

  it("detects UNC path", () => {
    expect(isPathQuery("\\\\server\\share")).toBe(true);
    expect(isPathQuery("\\\\server\\share\\folder")).toBe(true);
    // 正規化済み UNC（to_slash 後）は startsWith('/') で検出される
    expect(isPathQuery("//server/share")).toBe(true);
    expect(isPathQuery("//server/share/folder")).toBe(true);
  });

  it("detects %VAR% style env var path", () => {
    expect(isPathQuery("%USERPROFILE%\\app")).toBe(true);
    expect(isPathQuery("%APPDATA%/Roaming")).toBe(true);
    expect(isPathQuery("%USERPROFILE%")).toBe(true);
  });

  it("rejects unclosed percent sign", () => {
    expect(isPathQuery("%NOTAVAR")).toBe(false);
  });

  it("detects $VAR style env var path", () => {
    expect(isPathQuery("$HOME/app")).toBe(true);
    expect(isPathQuery("$XDG_CONFIG_HOME/foo")).toBe(true);
    expect(isPathQuery("$_VAR/bar")).toBe(true);
  });

  it("detects ${VAR} style env var path", () => {
    expect(isPathQuery("${HOME}/app")).toBe(true);
    expect(isPathQuery("${XDG_DATA_HOME}/foo")).toBe(true);
  });

  it("rejects lone dollar sign", () => {
    expect(isPathQuery("$")).toBe(false);
    expect(isPathQuery("$1nvalid")).toBe(false);
    expect(isPathQuery("${")).toBe(false);
    expect(isPathQuery("${HOME")).toBe(false);
  });

  it("detects shell: special folder", () => {
    expect(isPathQuery("shell:startup")).toBe(true);
    expect(isPathQuery("shell:desktop")).toBe(true);
    expect(isPathQuery("shell:RecycleBinFolder")).toBe(true);
    expect(isPathQuery("SHELL:startup")).toBe(true);
  });
});

// --- matchKey ---

function evt(key, { ctrlKey = false, altKey = false, shiftKey = false, metaKey = false } = {}) {
  return { key, ctrlKey, altKey, shiftKey, metaKey };
}

describe("matchKey", () => {
  it("matches simple key Enter", () => {
    expect(matchKey(evt("Enter"), "Enter")).toBe(true);
  });

  it("rejects wrong key", () => {
    expect(matchKey(evt("Escape"), "Enter")).toBe(false);
  });

  it("matches Ctrl+n", () => {
    expect(matchKey(evt("n", { ctrlKey: true }), "Ctrl+n")).toBe(true);
  });

  it("rejects Ctrl+n when ctrl not held", () => {
    expect(matchKey(evt("n"), "Ctrl+n")).toBe(false);
  });

  it("matches Alt+Space via Space mapping", () => {
    expect(matchKey(evt(" ", { altKey: true }), "Alt+Space")).toBe(true);
  });

  it("rejects Alt+Space when alt not held", () => {
    expect(matchKey(evt(" "), "Alt+Space")).toBe(false);
  });

  it("is case-insensitive on key part", () => {
    expect(matchKey(evt("f", { ctrlKey: true }), "Ctrl+F")).toBe(true);
    expect(matchKey(evt("F", { ctrlKey: true }), "Ctrl+f")).toBe(true);
  });

  it("matches Ctrl+Shift+P", () => {
    expect(matchKey(evt("P", { ctrlKey: true, shiftKey: true }), "Ctrl+Shift+P")).toBe(true);
  });

  it("rejects Ctrl+Shift+P when shift missing", () => {
    expect(matchKey(evt("p", { ctrlKey: true }), "Ctrl+Shift+P")).toBe(false);
  });

  it("matches Escape", () => {
    expect(matchKey(evt("Escape"), "Escape")).toBe(true);
  });

  it("matches Tab", () => {
    expect(matchKey(evt("Tab"), "Tab")).toBe(true);
  });

  it("handles Meta alias", () => {
    expect(matchKey(evt("k", { metaKey: true }), "Meta+k")).toBe(true);
  });

  it("handles Cmd alias for Meta", () => {
    expect(matchKey(evt("k", { metaKey: true }), "Cmd+k")).toBe(true);
  });

  it("rejects when extra modifier held", () => {
    expect(matchKey(evt("n", { ctrlKey: true, shiftKey: true }), "Ctrl+n")).toBe(false);
  });

  it("matches Ctrl+f (accept_word default)", () => {
    expect(matchKey(evt("f", { ctrlKey: true }), "Ctrl+f")).toBe(true);
  });

  it("matches Ctrl+e (accept_line default)", () => {
    expect(matchKey(evt("e", { ctrlKey: true }), "Ctrl+e")).toBe(true);
  });

  it("matches Ctrl+w (delete_word default)", () => {
    expect(matchKey(evt("w", { ctrlKey: true }), "Ctrl+w")).toBe(true);
  });

  it("matches Ctrl+u (delete_line default)", () => {
    expect(matchKey(evt("u", { ctrlKey: true }), "Ctrl+u")).toBe(true);
  });
});

// --- shouldBypassTemplate ---

describe("shouldBypassTemplate", () => {
  const memoNewItem = {
    name: "MemoNew",
    args: ['~/memo/{{ now() | date(format="%Y%m%d") }}-{{ args }}.md'],
  };
  const memoListItem = {
    name: "MemoList",
    args: [],
  };
  const gitItem = {
    name: "git checkout",
    args: ["checkout", "{{ args }}"],
  };
  const noArgsItem = {
    name: "plain",
    args: [],
  };

  // ケース1: history エントリ + テンプレート args → バイパスすべき (MemoNew の主要ケース)
  it("bypasses when candidate is from history and argItem has template args", () => {
    const history = ["~/memo/20260321-hoge.md", "~/memo/20260320-rust.md"];
    expect(shouldBypassTemplate("~/memo/20260321-hoge.md", history, memoNewItem)).toBe(true);
  });

  // ケース2: history エントリ + テンプレート args (git checkout)
  it("bypasses when candidate is from history and argItem has {{ args }} template", () => {
    const history = ["main", "feature/my-branch"];
    expect(shouldBypassTemplate("main", history, gitItem)).toBe(true);
  });

  // ケース3: history エントリだが argItem にテンプレートなし → バイパスしない
  it("does not bypass when candidate is from history but argItem has no template", () => {
    const history = ["some-arg"];
    expect(shouldBypassTemplate("some-arg", history, noArgsItem)).toBe(false);
  });

  // ケース4: history エントリだが MemoList (args が空配列) → バイパスしない
  it("does not bypass when candidate is from history but argItem args is empty", () => {
    const history = ["~/memo/20260321-hoge.md"];
    expect(shouldBypassTemplate("~/memo/20260321-hoge.md", history, memoListItem)).toBe(false);
  });

  // ケース5: 補完候補は history にない (path 補完など) + テンプレートあり → バイパスしない
  it("does not bypass when candidate is not from history even if argItem has template", () => {
    const history = ["~/memo/20260320-old.md"];
    expect(shouldBypassTemplate("~/memo/20260321-new.md", history, memoNewItem)).toBe(false);
  });

  // ケース6: 空の history + テンプレートあり → バイパスしない
  it("does not bypass when historyArgs is empty", () => {
    expect(shouldBypassTemplate("~/memo/20260321-hoge.md", [], memoNewItem)).toBe(false);
  });

  // ケース7: argItem が undefined → バイパスしない
  it("does not bypass when argItem is undefined", () => {
    const history = ["some-value"];
    expect(shouldBypassTemplate("some-value", history, undefined)).toBe(false);
  });

  // ケース8: argItem.args が undefined → バイパスしない
  it("does not bypass when argItem.args is undefined", () => {
    const history = ["some-value"];
    expect(shouldBypassTemplate("some-value", history, { name: "x" })).toBe(false);
  });

  // ケース9: 複数 args のうち1つだけテンプレートを含む → バイパスする
  it("bypasses when any arg contains {{ even if others do not", () => {
    const item = { name: "multi", args: ["--output", "~/out/{{ args }}.txt"] };
    const history = ["myfile"];
    expect(shouldBypassTemplate("myfile", history, item)).toBe(true);
  });

  // ケース10: fuzzy マッチしても history に含まれない文字列 → バイパスしない
  it("does not bypass a string that fuzzy-matches history but is not identical", () => {
    const history = ["~/memo/20260321-hoge.md"];
    // "hoge" は fuzzy match するが includes() には引っかからない
    expect(shouldBypassTemplate("hoge", history, memoNewItem)).toBe(false);
  });
});

// --- getEffectiveSearchMode ---

const MODES = ["fuzzy", "exact", "migemo"];

describe("getEffectiveSearchMode", () => {
  it("returns uiSearchMode in search mode regardless of overrides", () => {
    expect(getEffectiveSearchMode("search", "migemo", "exact", "fuzzy")).toBe("fuzzy");
  });

  it("returns uiSearchMode in args mode when no overrides", () => {
    expect(getEffectiveSearchMode("args", null, null, "fuzzy")).toBe("fuzzy");
  });

  it("returns completion_search_mode over uiSearchMode in args mode", () => {
    expect(getEffectiveSearchMode("args", null, "exact", "fuzzy")).toBe("exact");
  });

  it("returns argsModeSearchOverride over completion_search_mode in args mode", () => {
    expect(getEffectiveSearchMode("args", "migemo", "exact", "fuzzy")).toBe("migemo");
  });

  it("returns argsModeSearchOverride over uiSearchMode when no completion_search_mode", () => {
    expect(getEffectiveSearchMode("args", "exact", null, "fuzzy")).toBe("exact");
  });

  it("uiSearchMode change is reflected when in search mode", () => {
    expect(getEffectiveSearchMode("search", null, "exact", "migemo")).toBe("migemo");
  });
});

// --- nextSearchMode ---

describe("nextSearchMode", () => {
  it("cycles fuzzy → exact → migemo → fuzzy", () => {
    expect(nextSearchMode("fuzzy", MODES)).toBe("exact");
    expect(nextSearchMode("exact", MODES)).toBe("migemo");
    expect(nextSearchMode("migemo", MODES)).toBe("fuzzy");
  });

  it("wraps around at the end", () => {
    expect(nextSearchMode("migemo", MODES)).toBe("fuzzy");
  });

  it("works with unknown current (falls back to first)", () => {
    // indexOf returns -1, so (−1 + 1) % 3 = 0
    expect(nextSearchMode("unknown", MODES)).toBe("fuzzy");
  });
});

// --- fuzzyMatch ---

describe("fuzzyMatch", () => {
  it("empty query always matches", () => {
    expect(fuzzyMatch("", "anything")).toBe(true);
    expect(fuzzyMatch("", "")).toBe(true);
  });
  it("exact match", () => {
    expect(fuzzyMatch("abc", "abc")).toBe(true);
  });
  it("subsequence match", () => {
    expect(fuzzyMatch("rust", "20260321-rust-notes.md")).toBe(true);
    expect(fuzzyMatch("notes", "20260321-rust-notes.md")).toBe(true);
    expect(fuzzyMatch("26nt", "20260321-rust-notes.md")).toBe(true);
  });
  it("case insensitive", () => {
    expect(fuzzyMatch("RUST", "20260321-rust-notes.md")).toBe(true);
    expect(fuzzyMatch("Notes", "20260321-rust-notes.md")).toBe(true);
  });
  it("no match when characters missing", () => {
    expect(fuzzyMatch("xyz", "20260321-rust-notes.md")).toBe(false);
    expect(fuzzyMatch("rustz", "rust-notes.md")).toBe(false);
  });
  it("prefix match works", () => {
    expect(fuzzyMatch("2026", "20260321-notes.md")).toBe(true);
  });
});

// --- makePathItem ---

describe("makePathItem", () => {
  it("creates a Path item with the given path as both name and path", () => {
    const item = makePathItem("~/Documents");
    expect(item.name).toBe("~/Documents");
    expect(item.path).toBe("~/Documents");
    expect(item.source).toBe("Path");
    expect(item.args).toEqual([]);
    expect(item.completion).toBe("none");
  });

  it("creates a Path item with empty args and workdir", () => {
    const item = makePathItem("C:/Users");
    expect(item.workdir).toBeNull();
    expect(item.completion_list).toEqual([]);
    expect(item.completion_command).toBeNull();
  });
});

// --- makeWarningItem ---

describe("makeWarningItem", () => {
  it("creates a Warning item with the given file and error", () => {
    const item = makeWarningItem("config.toml", "parse error at line 5");
    expect(item.name).toBe("config.toml");
    expect(item.path).toBe("config.toml");
    expect(item.source).toBe("Warning");
    expect(item._warning_error).toBe("parse error at line 5");
  });

  it("has no args and no completion", () => {
    const item = makeWarningItem("config.toml", "err");
    expect(item.args).toEqual([]);
    expect(item.completion).toBe("none");
    expect(item.completion_list).toEqual([]);
  });
});

// --- canHaveArgs ---

describe("canHaveArgs", () => {
  it("returns false for Url source", () => {
    expect(canHaveArgs({ source: "Url" })).toBe(false);
  });

  it("returns true for Path source (executables from scan_dirs can have args)", () => {
    expect(canHaveArgs({ source: "Path" })).toBe(true);
  });

  it("returns false for History source", () => {
    expect(canHaveArgs({ source: "History" })).toBe(false);
  });

  it("returns false for Warning source", () => {
    expect(canHaveArgs({ source: "Warning" })).toBe(false);
  });

  it("returns true for Config source", () => {
    expect(canHaveArgs({ source: "Config" })).toBe(true);
  });

  it("returns true for ScanDir source", () => {
    expect(canHaveArgs({ source: "ScanDir" })).toBe(true);
  });

  it("returns true for Apps source", () => {
    expect(canHaveArgs({ source: "Apps" })).toBe(true);
  });

  it("returns true for SlashCmd source", () => {
    expect(canHaveArgs({ source: "SlashCmd" })).toBe(true);
  });

  it("returns true for null (null?.source is undefined, not a blocked source)", () => {
    expect(canHaveArgs(null)).toBe(true);
  });

  it("returns true for undefined", () => {
    expect(canHaveArgs(undefined)).toBe(true);
  });
});

// --- validateKeybindings ---

describe("validateKeybindings", () => {
  it("returns no warnings for valid keybindings", () => {
    expect(validateKeybindings({
      launch: "Ctrl+Space",
      next: "Ctrl+n",
      prev: "Ctrl+p",
      confirm: "Enter",
      close: "Escape",
    })).toEqual([]);
  });

  it("returns no warnings for empty keybindings", () => {
    expect(validateKeybindings({})).toEqual([]);
  });

  it("skips null/empty bindings", () => {
    expect(validateKeybindings({ launch: null, next: "" })).toEqual([]);
  });

  it("warns for concatenated modifier without + (Ctrlp style)", () => {
    const warnings = validateKeybindings({ next: "Ctrlp" });
    expect(warnings).toHaveLength(1);
    expect(warnings[0][1]).toContain('did you mean "Ctrl+p"?');
  });

  it("suggests correct format for Shiftn", () => {
    const warnings = validateKeybindings({ confirm: "Shiftn" });
    expect(warnings[0][1]).toContain('did you mean "Shift+n"?');
  });

  it("warns for unknown modifier", () => {
    const warnings = validateKeybindings({ next: "Win+n" });
    expect(warnings).toHaveLength(1);
    expect(warnings[0][1]).toContain('unknown modifier "Win"');
  });

  it("returns warning referencing config.toml as the source file", () => {
    const warnings = validateKeybindings({ next: "Ctrlp" });
    expect(warnings[0][0]).toBe("config.toml");
  });

  it("warns for multiple bad bindings", () => {
    const warnings = validateKeybindings({
      next: "Ctrlp",
      prev: "Win+n",
    });
    expect(warnings).toHaveLength(2);
  });

  it("accepts Alt, Shift, Meta, Cmd as valid modifiers", () => {
    expect(validateKeybindings({
      a: "Alt+x",
      b: "Shift+Enter",
      c: "Meta+k",
      d: "Cmd+k",
    })).toEqual([]);
  });

  it("warns for Ctrl+Shift+Ctrlp style (concatenated in key position)", () => {
    const warnings = validateKeybindings({ cycle: "Ctrl+Shiftn" });
    expect(warnings[0][1]).toContain('did you mean');
  });
});

// --- normalizeConfigFileName ---

describe("normalizeConfigFileName", () => {
  it("adds config. prefix and .toml suffix for bare name", () => {
    expect(normalizeConfigFileName("hoge")).toBe("config.hoge.toml");
  });

  it("adds config. prefix when .toml already present", () => {
    expect(normalizeConfigFileName("hoge.toml")).toBe("config.hoge.toml");
  });

  it("leaves config.*.toml unchanged", () => {
    expect(normalizeConfigFileName("config.hoge.toml")).toBe("config.hoge.toml");
  });

  it("leaves config.toml unchanged", () => {
    expect(normalizeConfigFileName("config.toml")).toBe("config.toml");
  });

  it("adds .toml suffix to config.hoge without extension", () => {
    expect(normalizeConfigFileName("config.hoge")).toBe("config.hoge.toml");
  });

  it("handles local suffix", () => {
    expect(normalizeConfigFileName("local")).toBe("config.local.toml");
    expect(normalizeConfigFileName("local.toml")).toBe("config.local.toml");
    expect(normalizeConfigFileName("config.local.toml")).toBe("config.local.toml");
  });
});

// --- completionMatches ---

describe("completionMatches", () => {
  it("empty query always matches", () => {
    expect(completionMatches("", "anything", "fuzzy")).toBe(true);
    expect(completionMatches("", "anything", "exact")).toBe(true);
  });

  it("fuzzy mode: subsequence match", () => {
    expect(completionMatches("rnt", "rust-notes.md", "fuzzy")).toBe(true);
    expect(completionMatches("xyz", "rust-notes.md", "fuzzy")).toBe(false);
  });

  it("fuzzy mode: case insensitive", () => {
    expect(completionMatches("RUST", "rust-notes.md", "fuzzy")).toBe(true);
  });

  it("exact mode: substring match", () => {
    expect(completionMatches("notes", "rust-notes.md", "exact")).toBe(true);
    expect(completionMatches("notex", "rust-notes.md", "exact")).toBe(false);
  });

  it("exact mode: case insensitive", () => {
    expect(completionMatches("NOTES", "rust-notes.md", "exact")).toBe(true);
  });

  it("migemo mode with no instance falls back to fuzzy", () => {
    expect(completionMatches("rnt", "rust-notes.md", "migemo", null)).toBe(true);
    expect(completionMatches("xyz", "rust-notes.md", "migemo", null)).toBe(false);
  });

  it("migemo mode with throwing instance falls back to fuzzy", () => {
    const badMigemo = { query: () => { throw new Error("fail"); } };
    expect(completionMatches("rnt", "rust-notes.md", "migemo", badMigemo)).toBe(true);
  });

  it("migemo mode with valid instance uses regex", () => {
    const mockMigemo = { query: (q) => q }; // passthrough: query = regex pattern
    expect(completionMatches("rust", "rust-notes.md", "migemo", mockMigemo)).toBe(true);
    expect(completionMatches("^rust$", "rust", "migemo", mockMigemo)).toBe(true);
    expect(completionMatches("^rust$", "not-rust-here", "migemo", mockMigemo)).toBe(false);
  });

  it("defaults to fuzzy when completionMode is unrecognized", () => {
    expect(completionMatches("rnt", "rust-notes.md", "unknown")).toBe(true);
  });
});
