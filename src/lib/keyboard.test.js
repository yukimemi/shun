import { parse } from "svelte/compiler";
import { describe, expect, it, vi } from "vitest";
import { isImeKeydown } from "./utils.js";
import page from "../routes/+page.svelte?raw";

// Exercise the real shared entry point without importing the Tauri runtime.
const ast = parse(page);
let handler;
for (const node of ast.instance.content.body) {
  if (node.type === "FunctionDeclaration" && node.id.name === "onKeydown") handler = node;
}
if (!handler) throw new Error("Missing shared keyboard handler");
const createKeydown = new Function(
  "isImeKeydown", "helpVisible", "mode", "handleArgsKeydown", "handleSearchKeydown",
  `return (${page.slice(handler.start, handler.end)});`,
);

function keydown(overrides = {}) {
  return {
    key: "Enter", keyCode: 13, isComposing: false,
    shiftKey: false, ctrlKey: false, altKey: false, metaKey: false,
    preventDefault: vi.fn(), ...overrides,
  };
}

describe.each(["search", "args"])("IME keyboard routing in %s mode", (mode) => {
  function setup(helpVisible = false) {
    const args = vi.fn();
    const search = vi.fn();
    return {
      dispatch: createKeydown(isImeKeydown, helpVisible, mode, args, search),
      selected: mode === "args" ? args : search,
      other: mode === "args" ? search : args,
    };
  }

  it.each([
    ["Windows-style confirmation", { isComposing: true, keyCode: 229 }],
    ["standard composing confirmation", { isComposing: true, keyCode: 13 }],
    ["macOS confirmation after compositionend", { isComposing: false, keyCode: 229 }],
  ])("leaves %s alone and allows continued typing before submission", async (_, fields) => {
    const { dispatch, selected, other } = setup();
    const confirmation = keydown(fields);
    await dispatch(confirmation);
    expect(selected).not.toHaveBeenCalled();
    expect(confirmation.preventDefault).not.toHaveBeenCalled();

    // Continued typing must still reach the active mode.
    const typing = keydown({ key: "a", keyCode: 65 });
    await dispatch(typing);
    const submit = keydown();
    await dispatch(submit);
    expect(selected.mock.calls).toEqual([[typing], [submit]]);
    expect(other).not.toHaveBeenCalled();
  });

  it("routes an immediate second Enter exactly once after macOS confirmation", async () => {
    const { dispatch, selected } = setup();
    await dispatch(keydown({ keyCode: 229 }));
    const submit = keydown();
    await dispatch(submit);
    expect(selected.mock.calls).toEqual([[submit]]);
  });

  it.each([
    { key: "Tab" }, { key: "Escape" }, { shiftKey: true },
    { ctrlKey: true }, { altKey: true }, { metaKey: true },
  ])("protects IME shortcuts before help dismissal (%j)", async (fields) => {
    for (const helpVisible of [false, true]) {
      const { dispatch, selected, other } = setup(helpVisible);
      for (const signal of [{ isComposing: true }, { keyCode: 229 }]) {
        const event = keydown({ ...fields, ...signal });
        await dispatch(event);
        expect(event.preventDefault).not.toHaveBeenCalled();
      }
      expect(selected).not.toHaveBeenCalled();
      expect(other).not.toHaveBeenCalled();
    }
  });

  it("routes ordinary Enter and Shift+Enter without prior composition", async () => {
    const { dispatch, selected } = setup();
    const confirm = keydown();
    const runQuery = keydown({ shiftKey: true });
    await dispatch(confirm);
    await dispatch(runQuery);
    expect(selected.mock.calls).toEqual([[confirm], [runQuery]]);
  });
});
