import { describe, it, expect } from "vitest";
import { firstSepIdx, isPathQuery, matchKey, fuzzyMatch, shouldBypassTemplate } from "./utils.js";

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
