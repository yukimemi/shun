<script>
  let { keybindings, currentPreset, uiSearchMode, uiSortOrder } = $props();

  const modeLabel = (m) =>
    m === "fuzzy" ? "≋" : m === "exact" ? "―" : m === "migemo" ? "あ" :
    m === "fuzzy_migemo" ? "≋あ" : "―あ";
</script>

<div class="help-panel">
  <div class="help-section">
    <div class="help-row">
      <span class="help-label">theme</span>
      <span class="help-value">{currentPreset}</span>
    </div>
    <div class="help-row">
      <span class="help-label">search mode</span>
      <span class="help-value">{uiSearchMode} {modeLabel(uiSearchMode)}</span>
      <span class="help-key">{keybindings.cycle_search_mode}</span>
    </div>
    <div class="help-row">
      <span class="help-label">sort order</span>
      <span class="help-value">{uiSortOrder} {uiSortOrder === "count_first" ? "#" : "⌚"}</span>
      <span class="help-key">{keybindings.cycle_sort_order}</span>
    </div>
  </div>
  <div class="help-divider"></div>
  <div class="help-section">
    <div class="help-row">
      <span class="help-label">show / hide</span>
      <span class="help-key">{keybindings.launch}</span>
    </div>
    <div class="help-row">
      <span class="help-label">next / prev</span>
      <span class="help-key">{keybindings.next} / {keybindings.prev}</span>
    </div>
    <div class="help-row">
      <span class="help-label">launch</span>
      <span class="help-key">{keybindings.confirm}</span>
    </div>
    <div class="help-row">
      <span class="help-label">args mode</span>
      <span class="help-key">{keybindings.arg_mode}</span>
    </div>
    <div class="help-row">
      <span class="help-label">run query</span>
      <span class="help-key">{keybindings.run_query}</span>
    </div>
    <div class="help-row">
      <span class="help-label">accept word / line</span>
      <span class="help-key">{keybindings.accept_word} / {keybindings.accept_line}</span>
    </div>
    <div class="help-row">
      <span class="help-label">delete word / line</span>
      <span class="help-key">{keybindings.delete_word} / {keybindings.delete_line}</span>
    </div>
    <div class="help-row">
      <span class="help-label">delete item</span>
      <span class="help-key">{keybindings.delete_item}</span>
    </div>
    <div class="help-row">
      <span class="help-label">close</span>
      <span class="help-key">{keybindings.close}</span>
    </div>
    <div class="help-row">
      <span class="help-label">toggle preview</span>
      <span class="help-key">{keybindings.toggle_preview}</span>
    </div>
    <div class="help-row">
      <span class="help-label">preview scroll</span>
      <span class="help-key">{keybindings.preview_scroll_down} / {keybindings.preview_scroll_up}</span>
    </div>
  </div>
  <div class="help-footer">any key to close</div>
</div>

<style>
  .help-panel {
    padding: 8px 0 4px;
  }

  .help-section {
    padding: 2px 20px;
  }

  .help-row {
    display: flex;
    align-items: center;
    height: 30px;
    gap: 8px;
    font-size: calc(var(--font-size, 14px) - 1px);
  }

  .help-label {
    width: 110px;
    flex-shrink: 0;
    color: var(--color-muted, #585b70);
  }

  .help-value {
    flex: 1;
    color: var(--color-text, #cdd6f4);
  }

  .help-key {
    margin-left: auto;
    color: var(--color-blue, #89b4fa);
    font-family: monospace;
    font-size: calc(var(--font-size, 14px) - 2px);
    background: var(--color-surface, #313244);
    padding: 2px 6px;
    border-radius: 4px;
  }

  .help-divider {
    height: 1px;
    background: var(--color-surface, #313244);
    margin: 8px 20px;
  }

  .help-footer {
    padding: 4px 20px 6px;
    font-size: calc(var(--font-size, 14px) - 3px);
    color: var(--color-overlay, #45475a);
    text-align: right;
  }
</style>
