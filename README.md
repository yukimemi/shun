<div align="center">
  <img src="src-tauri/icons/icon.png" width="128" alt="shun icon" />

  # shun (瞬)

  > A cross-platform, keyboard-driven minimal launcher — like Alfred/Raycast, but yours.
</div>

<div align="center">

[![Release](https://img.shields.io/github/v/release/yukimemi/shun?style=flat-square)](https://github.com/yukimemi/shun/releases)
[![License](https://img.shields.io/github/license/yukimemi/shun?style=flat-square)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri-blue?style=flat-square)](https://tauri.app)

</div>

<div align="center">
  <img src="https://github.com/yukimemi/shun/releases/download/v1.1.1/shun.gif" alt="shun demo" width="640" />
</div>

## Features

- **Instant popup** — global hotkey brings up the launcher anywhere; appears on cursor's monitor by default, or fixed to `"primary"` / index via `monitor` config
- **Fuzzy / exact / migemo / combined search** — fuzzy via [nucleo-matcher](https://github.com/helix-editor/nucleo); migemo via [rustmigemo](https://github.com/oguna/rustmigemo) / [jsmigemo](https://github.com/oguna/jsmigemo) (type `hajime` to match `初めて`); `fuzzy_migemo` / `exact_migemo` return union of both
- **Status badges** — subtle pill in the input corner shows current search mode (`≋` / `―` / `あ` / `≋あ` / `―あ`) and sort order (`#` / `⌚`); click or use keybindings to cycle
- **Args mode** — press `Tab` to pass extra arguments; path / list / command completion with ghost text
- **Launch history** — frecency sorting; previous args remembered and suggested as ghost text
- **File preview panel** — `Ctrl+Shift+p` toggles a side panel with Shiki syntax highlighting; scroll with `Ctrl+j` / `Ctrl+k`
- **Slash commands** — `/reload`, `/config`, `/update`, `/theme`, `/save`, `/reset` and more
- **Auto-update** — detects new releases on startup; one command to download, install, and restart
- **Theming** — built-in presets (Catppuccin, Nord, Dracula, Tokyo Night) + per-color overrides
- **Movable window** — drag the grip at the top of the launcher to reposition; save with `/save position`
- **Cross-platform** — Windows, macOS, Linux; portable zip with no admin rights required

## Installation

**Windows** (PowerShell, no admin required)
```powershell
irm https://yukimemi.github.io/shun/install.ps1 | iex
```

**macOS / Linux**
```bash
curl -fsSL https://yukimemi.github.io/shun/install.sh | sh
```

<details>
<summary>Package managers & direct download</summary>

**Windows — winget**
```powershell
winget install yukimemi.shun
```

**Windows — Scoop**
```powershell
scoop bucket add yukimemi https://github.com/yukimemi/scoop-bucket
scoop install yukimemi/shun
```

**macOS — Homebrew**
```bash
brew tap yukimemi/tap
brew install --cask yukimemi/tap/shun
```

**Direct download** — latest installer from [Releases](https://github.com/yukimemi/shun/releases):

| Platform | File |
|---|---|
| Windows | `.msi` (recommended) or `shun-windows-x64.zip` (portable, no admin required) |
| macOS | `.dmg` (universal — Apple Silicon + Intel) |
| Linux | `.AppImage` or `.deb` |

</details>

## Quick start

Works out of the box with zero config. Config file is created automatically on first launch:

| Platform | Path |
|---|---|
| Windows | `%APPDATA%\shun\config.toml` |
| macOS | `~/Library/Application Support/shun/config.toml` |
| Linux | `~/.config/shun/config.toml` |

A minimal config to get started (non-existent paths are silently ignored):

```toml
[keybindings]
launch = "Ctrl+Space"   # global hotkey to show/hide

[[apps]]
name       = "Neovide"
path       = "neovide"
completion = "path"

# Windows
[[scan_dirs]]
path       = "~/bin"
extensions = ["exe", "bat", "ps1", "cmd"]

# macOS / Linux
[[scan_dirs]]
path       = "~/.local/bin"
extensions = ["sh", "py"]
```

After editing config, run `/reload` to apply all changes without restarting.

## Slash commands

| Command | Action |
|---|---|
| `/reload` | Reload config — re-registers global shortcut, rescans apps, re-applies all settings |
| `/config` | Open a config file (`Tab` to pick; `delete_item` key to delete; creates new `config.*.toml` if typed manually) |
| `/theme <name>` | Switch theme for this session (`Tab` to pick; set in `config.toml` to persist) |
| `/update` | Install latest release (shows version if update available) |
| `/history` | Open history file in default editor |
| `/version` | Show current version |
| `/save` | Save a setting to `config.local.toml` (`Tab` to pick: `monitor`, `position`, `theme`, `search_mode`, `sort_order`) |
| `/reset` | Reset a setting in `config.local.toml` — falls back to `config.toml` or default (`Tab` to pick) |
| `/help` | Show keybindings & current status (theme, search mode, sort order) |
| `/exit` | Quit shun |

<details>
<summary>Full configuration reference</summary>

### All options

```toml
# Search mode: "fuzzy" (default) | "exact" | "migemo" | "fuzzy_migemo" | "exact_migemo"
search_mode = "fuzzy"

# Sort order: "count_first" (default) | "recent_first"
sort_order = "count_first"

# Auto-hide when the launcher loses focus
hide_on_blur = false

# Start automatically at login (default: true); set to false to disable
auto_start = true

# Update check interval in seconds (0 to disable)
update_check_interval = 3600

# Launcher window width in pixels (default: 620)
window_width = 620

# Max items shown in the results list (default: 8)
max_items = 8

# Max items shown in the completion dropdown (default: 6)
max_completions = 6

# Font size in pixels (default: 14)
# font_size = 14

# Window opacity 0.0–1.0 (default: 1.0)
# opacity = 1.0

# Maximum number of history entries to keep (default: 1000)
# history_max_items = 1000

# Status badge icon style: "unicode" (default) | "svg"
# icon_style = "unicode"

# Monitor to show the launcher on: "cursor" (default) | "primary" | 0 | 1 | ...
# Useful in config.local.toml for per-machine override
# monitor = "cursor"

# Saved window position (set via /save position; delete to reset to default centering)
# position_x = 960.0
# position_y = 300.0

# Preview panel: show when browsing files in args mode (default: true)
# preview_args = true

# Preview panel: show when navigating search results (default: true)
# preview_search = true

# Preview panel width in pixels (default: 400)
# preview_width = 400

# Max lines to load for file preview (default: 500)
# max_preview_lines = 500

# Note: preview panel height is fixed to max_items × item height when visible,
# so increasing max_items also increases the preview panel height.

[keybindings]
launch      = "Ctrl+Space"   # Global hotkey to show/hide
next        = "Ctrl+n"
prev        = "Ctrl+p"
confirm     = "Enter"
arg_mode    = "Tab"
accept_word = "Ctrl+f"      # Accept next word/segment of ghost text
accept_line = "Ctrl+e"      # Accept full ghost text
delete_word = "Ctrl+w"      # Delete word before cursor
delete_line = "Ctrl+u"      # Delete to beginning of line
run_query   = "Shift+Enter" # Run typed query directly (skip history results)
close             = "Escape"
delete_item       = "Ctrl+d"        # Delete selected history/config item
cycle_search_mode = "Ctrl+Shift+m"  # Cycle search mode (fuzzy → exact → migemo)
cycle_sort_order  = "Ctrl+Shift+o"  # Cycle sort order (count_first ↔ recent_first)
toggle_preview    = "Ctrl+Shift+p"  # Toggle file preview panel
preview_scroll_down = "Ctrl+j"      # Scroll preview panel down
preview_scroll_up   = "Ctrl+k"      # Scroll preview panel up

# Logging
[log]
level = "warn"            # "debug" | "info" | "warn" (default) | "error" | "off"
max_file_size_kb = 1024
rotation = "keep_one"     # "keep_one" (default) | "keep_all" | number

# Theme
[theme]
preset = "catppuccin-mocha"   # "catppuccin-mocha" | "catppuccin-latte" | "nord" | "dracula" | "tokyo-night" | "one-half-dark" | "solarized-dark" | "solarized-light"
# bg      = "#1e1e2e"
# surface = "#313244"
# overlay = "#45475a"
# muted   = "#585b70"
# text    = "#cdd6f4"
# blue    = "#89b4fa"
# purple  = "#cba6f7"
# green   = "#a6e3a1"
# red     = "#f38ba8"

# Open editor with file path completion
[[apps]]
name       = "Neovide"
path       = "neovide"
completion = "path"       # "path" | "none" | "list" | "command"

# Search Google (Tab → type query → Enter)
[[apps]]
name = "Google"
path = "https://www.google.com/search?q={{ args | urlencode }}"

# docker exec with container name completion
[[apps]]
name               = "docker exec"
path               = "docker"
args               = ["exec", "-it"]
completion         = "command"
completion_command = "docker ps --format '{{.Names}}'"

# git checkout with branch completion (exact search for this app)
[[apps]]
name                   = "git checkout"
path                   = "git"
args                   = ["checkout"]
completion             = "command"
completion_command     = "git branch --format='%(refname:short)'"
completion_search_mode = "exact"  # "fuzzy" | "exact" | "migemo" (overrides global)
workdir                = "~/src/myproject"

# Override completion for scan_dirs items
[[overrides]]
name            = "scoop"
completion      = "list"
completion_list = ["install", "uninstall", "update", "search", "info"]

# Auto-register scripts from a directory
[[scan_dirs]]
path       = "~/.local/bin"
recursive  = false
extensions = ["sh", "py", "ps1", "cmd"]
```

### Override files (`config.*.toml`)

Any `config.*.toml` files in the same directory as `config.toml` are loaded and merged in alphabetical order after the base config. Useful for per-machine or per-context overrides with chezmoi or other dotfile managers.

Examples: `config.local.toml`, `config.work.toml`, `config.home.toml`

| Platform | Directory |
|---|---|
| Windows | `%APPDATA%\shun\` |
| macOS | `~/Library/Application Support/shun/` |
| Linux | `~/.config/shun/` |

Merge rules:

| Field | Behavior |
|---|---|
| `apps`, `scan_dirs`, `overrides` | Entries are **appended** |
| `search_mode`, `sort_order`, `hide_on_blur`, `auto_start`, `font_size`, `opacity`, `monitor`, etc. | Local value **overrides** (only when explicitly set) |
| `[keybindings]`, `[theme]`, `[log]` | **Per-field override** — only specified keys are overridden |

</details>

<details>
<summary>Template placeholders in <code>path</code> and <code>args</code></summary>

You can use [Tera](https://keats.github.io/tera/) template syntax in the `path` and `args` fields. Templates are evaluated at launch time when args are provided via Args mode (`Tab`).

**Context variables:**

| Variable | Value |
|---|---|
| `{{ args }}` | Extra args joined by space (raw) |
| `{{ args_list }}` | Extra args as an array |
| `{{ env.VAR_NAME }}` | Environment variable |
| `{{ vars.my_var }}` | User-defined variable from `[vars]` in config |

**Useful Tera expressions:**

| Expression | Result |
|---|---|
| `{{ args \| urlencode }}` | URL-encoded (spaces → `%20`) |
| `{{ now() \| date(format="%Y%m%d") }}` | Today's date e.g. `20260321` |
| `{{ now() \| date(format="%Y-%m-%d") }}` | Today's date e.g. `2026-03-21` |
| `{{ get_env(name="VAR", default="fallback") }}` | Env var with default |

**Example — Web search:**

```toml
[[apps]]
name = "Google"
path = "https://www.google.com/search?q={{ args | urlencode }}"
```
Open shun → `goo` → `Tab` → `rust borrow checker` → `Enter` → opens the search. Query is saved to history.

**Example — Date-stamped memos:**

```toml
# MemoNew: Tab → type title → Enter → opens "20260321-title.md"
[[apps]]
name       = "MemoNew"
path       = "nvim"
args       = ['~/memo/{{ now() | date(format="%Y%m%d") }}-{{ args }}.md']
completion = "none"

# MemoList: Tab → pick existing memo with migemo path completion → Enter
# (type "hajime" to find "初めて.md")
[[apps]]
name                   = "MemoList"
path                   = "nvim"
args                   = ["~/memo/{{ args }}"]
completion             = "path"
completion_search_mode = "migemo"
```

**Example — User-defined variables:**

```toml
[vars]
src_dir = "~/src/github.com/yourname"

[[apps]]
name       = "Open Project"
path       = "neovide"
args       = ["{{ vars.src_dir }}/{{ args }}"]
completion = "path"
```

</details>

<details>
<summary>Theming</summary>

Switch theme for the current session with `/theme <name>` (`Tab` to pick). To persist across restarts, set it in `config.toml`:

```toml
[theme]
preset = "nord"
bg     = "#1a1a2e"  # optional per-color override
```

**Built-in presets:**

| Preset | Description |
|---|---|
| `catppuccin-mocha` | Default — dark, muted pastel |
| `catppuccin-latte` | Light variant of Catppuccin |
| `nord` | Arctic, blue-toned dark |
| `dracula` | Dark with vibrant purple/pink |
| `tokyo-night` | Dark blue-purple, Tokyo Night style |
| `one-half-dark` | One Half Dark (as used in VS Code / Windows Terminal) |
| `solarized-dark` | Solarized Dark |
| `solarized-light` | Solarized Light |

**Color keys** (all optional — unset keys fall back to the preset):

| Key | Role |
|---|---|
| `bg` | Launcher background |
| `surface` | Selected item background / border |
| `overlay` | Ghost text / separator / count |
| `muted` | Placeholder / source labels |
| `text` | Main text |
| `blue` | Accent — dirs, URLs, args app name |
| `purple` | Slash commands |
| `green` | Path items |
| `red` | History items |

</details>

<details>
<summary>Keybindings reference</summary>

All keybindings are configurable via `[keybindings]` in `config.toml`. Changes take effect after `/reload` — no restart needed.

- In-app keys use [KeyboardEvent.key values](https://developer.mozilla.org/en-US/docs/Web/API/UI_Events/Keyboard_event_key_values)
- The global `launch` shortcut uses [global-hotkey key codes](https://docs.rs/global-hotkey/latest/global_hotkey/hotkey/enum.Code.html)

**Search mode** (default keys):

| Key | Action |
|---|---|
| `Ctrl+Space` | Show / hide launcher |
| `Ctrl+n` / `↓` | Next item |
| `Ctrl+p` / `↑` | Previous item |
| `Enter` | Launch selected item |
| `Shift+Enter` | Launch typed query directly (skip history results) |
| `Tab` | Enter args mode |
| `Ctrl+f` | Accept next word of ghost text |
| `Ctrl+e` | Accept full ghost text |
| `Ctrl+w` | Delete word before cursor |
| `Ctrl+u` | Delete to beginning of line |
| `Ctrl+d` | Delete selected history item |
| `Ctrl+Shift+m` | Cycle search mode (fuzzy → exact → migemo) |
| `Ctrl+Shift+o` | Cycle sort order (count_first ↔ recent_first) |
| `Ctrl+Shift+p` | Toggle file preview panel |
| `Ctrl+j` | Scroll preview panel down |
| `Ctrl+k` | Scroll preview panel up |
| `Escape` | Hide launcher |

**Args mode** (default keys):

| Key | Action |
|---|---|
| `Enter` | Launch with args |
| `Shift+Enter` | Launch typed query directly |
| `Tab` | Apply selected completion |
| `Ctrl+n` / `Ctrl+p` | Navigate completion list |
| `Ctrl+f` / `Ctrl+e` | Accept ghost text (word / full) |
| `Ctrl+w` | Delete word before cursor |
| `Ctrl+u` | Delete to beginning of line |
| `Ctrl+d` | Delete selected history completion |
| `Ctrl+Shift+m` | Cycle search mode |
| `Ctrl+Shift+o` | Cycle sort order |
| `Ctrl+Shift+p` | Toggle file preview panel |
| `Ctrl+j` / `Ctrl+k` | Scroll preview panel down / up |
| `Escape` | Back to search |

**Special input:**

| Input | Action |
|---|---|
| `https://...` | Open URL in default browser |
| `~/...`, `C:/...` | Browse filesystem, open in file manager |

</details>

<details>
<summary>Building from source</summary>

```bash
# Prerequisites: Node.js, Rust

git clone https://github.com/yukimemi/shun
cd shun
npm install
npm run tauri dev     # development
npm run tauri build   # production build
```

</details>

## Credits

shun's migemo feature is powered by the following libraries and data by [oguna](https://github.com/oguna):

| Component | Role | License |
|---|---|---|
| [rustmigemo](https://github.com/oguna/rustmigemo) | Rust migemo engine | MIT |
| [jsmigemo](https://github.com/oguna/jsmigemo) | JavaScript migemo engine | MIT |
| [yet-another-migemo-dict](https://github.com/oguna/yet-another-migemo-dict) | Bundled compact dictionary (Mozc + UniDic based) | BSD-3-Clause |

Full license texts are in the [NOTICE](NOTICE) file.

## License

MIT
