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

- **Instant popup** — global hotkey brings up the launcher anywhere
- **Fuzzy / exact search** — powered by [nucleo-matcher](https://github.com/helix-editor/nucleo) (the same engine as Helix editor)
- **Launch history** — frecency sorting (count-first or recent-first)
- **Args mode** — press `Tab` to pass extra arguments to any app
- **Path & URL completion** — ghost text + dropdown, navigate with `Ctrl-n/p`, confirm with `Ctrl-f/e`
- **Args history** — previous argument combinations remembered and suggested as ghost text
- **Configurable completion** — `path` / `list` / `command` / `none` per app
- **URL & path navigation** — type `https://...` or `~/...` to open directly
- **Slash commands** — `/exit`, `/config`, `/rescan`, `/update`
- **Auto-update** — checks for new releases on startup; install in one keystroke with download progress
- **Portable friendly** — portable zip includes self-update (no admin rights required)
- **Theming** — built-in presets (Catppuccin, Nord, Dracula, Tokyo Night) + per-color overrides via `[theme]` in config
- **Font size & opacity** — configurable via `font_size` and `opacity` in config
- **History limit** — cap history entries with `history_max_items` (default: 1000)
- **Configurable logging** — set log level (`debug` / `info` / `warn` / `error` / `off`) via `[log]` in config; log file at `%APPDATA%\shun\logs\shun.log`
- **Local config override** — `config.local.toml` merges machine-specific settings without touching the main config (chezmoi-friendly)
- **Auto-hide on blur** — optionally hide when focus leaves the launcher
- **Multi-monitor** — appears on the monitor where your cursor is
- **Minimal UI** — borderless, transparent, always-on-top
- **Cross-platform** — Windows, macOS, Linux

## Installation

### One-liner install

**Windows** (PowerShell, no admin required)
```powershell
irm https://yukimemi.github.io/shun/install.ps1 | iex
```

**macOS / Linux**
```bash
curl -fsSL https://yukimemi.github.io/shun/install.sh | sh
```

### Package managers

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

### Direct download

Download the latest installer from [Releases](https://github.com/yukimemi/shun/releases).

| Platform | File |
|---|---|
| Windows | `.msi` (recommended) or `shun-windows-x64.zip` (portable, no admin required) |
| macOS | `.dmg` (universal — Apple Silicon + Intel) |
| Linux | `.AppImage` or `.deb` |

## Configuration

Config file is created automatically on first launch:

| Platform | Path |
|---|---|
| Windows | `%APPDATA%\shun\config.toml` |
| macOS | `~/Library/Application Support/shun/config.toml` |
| Linux | `~/.config/shun/config.toml` |

### Local override file

Place a `config.local.toml` in the same directory as `config.toml` to add machine-specific settings without modifying the main file. This is useful when managing `config.toml` with a dotfile manager (e.g. chezmoi) while keeping local overrides out of version control.

| Platform | Path |
|---|---|
| Windows | `%APPDATA%\shun\config.local.toml` |
| macOS | `~/Library/Application Support/shun/config.local.toml` |
| Linux | `~/.config/shun/config.local.toml` |

**Merge rules:**

| Field | Behavior |
|---|---|
| `apps`, `scan_dirs`, `overrides` | Entries are **appended** to the main config |
| `search_mode`, `sort_order`, `hide_on_blur`, `update_check_interval`, `font_size`, `opacity`, `history_max_items` | Local value **overrides** main (only when explicitly set) |
| `[keybindings]` | **Per-field override** — only specified keys are overridden |
| `[theme]` | **Per-field override** — `preset` and individual colors can be overridden independently |
| `[log]` | **Per-field override** — only specified fields are overridden |

**Example `config.local.toml`:**

```toml
# Machine-specific scan directories
[[scan_dirs]]
path = "C:/work/projects"
recursive = true
extensions = ["exe", "bat", "ps1"]

[[apps]]
name = "Internal Tool"
path = "C:/tools/internal.exe"
```

### Example `config.toml`

```toml
# Search mode: "fuzzy" (default) | "exact"
search_mode = "fuzzy"

# Sort order: "count_first" (default) | "recent_first"
sort_order = "count_first"

# Auto-hide when the launcher loses focus
hide_on_blur = false

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

[keybindings]
launch      = "Alt+Space"   # Global hotkey to show/hide
next        = "Ctrl+n"
prev        = "Ctrl+p"
confirm     = "Enter"
arg_mode    = "Tab"
accept_word = "Ctrl+f"      # Accept next word/segment of ghost text
accept_line = "Ctrl+e"      # Accept full ghost text
delete_word = "Ctrl+w"      # Delete word before cursor
delete_line = "Ctrl+u"      # Delete to beginning of line
run_query   = "Shift+Enter" # Run typed query directly (skip history results)
close       = "Escape"
delete_item = "Ctrl+d"      # Delete selected history item

# Logging — level applied to both Rust and JS (via tauri-plugin-log)
[log]
level = "warn"            # "debug" | "info" | "warn" (default) | "error" | "off"
max_file_size_kb = 1024   # rotate when log exceeds this size (default: 1024 = 1 MB)
rotation = "keep_one"     # "keep_one" (default) | "keep_all" | number (e.g. 5 to keep 5 files)

# Theme — preset + optional per-color overrides
[theme]
preset = "catppuccin-mocha"   # "catppuccin-mocha" | "catppuccin-latte" | "nord" | "dracula" | "tokyo-night"
# bg      = "#1e1e2e"         # background
# surface = "#313244"         # selected item / border
# overlay = "#45475a"         # ghost text / separator
# muted   = "#585b70"         # placeholder / labels
# text    = "#cdd6f4"         # main text
# blue    = "#89b4fa"         # accent (dirs, URLs, args app name)
# purple  = "#cba6f7"         # slash commands
# green   = "#a6e3a1"         # Path items
# red     = "#f38ba8"         # History items

# Open editor with file path completion
[[apps]]
name       = "Neovide"
path       = "neovide"
completion = "path"       # "path" | "none" | "list" | "command"

# Open a URL directly
[[apps]]
name = "GitHub"
path = "https://github.com"

# Search Google with Tab → type query → Enter ({{ args | urlencode }} URL-encodes the input)
[[apps]]
name = "Google"
path = "https://www.google.com/search?q={{ args | urlencode }}"

# Search GitHub code
[[apps]]
name = "GitHub Search"
path = "https://github.com/search?q={{ args | urlencode }}"

# Open a specific note by name ({{ args }} is substituted as-is)
[[apps]]
name = "Note"
path = "notepad"
args = ["C:/notes/{{ args }}.md"]

# Run with free-form arguments (no completion)
[[apps]]
name       = "systemctl"
path       = "systemctl"
completion = "none"

# docker exec into a running container (completion from docker ps)
[[apps]]
name               = "docker exec"
path               = "docker"
args               = ["exec", "-it"]
completion         = "command"
completion_command = "docker ps --format '{{.Names}}'"

# git checkout with branch name completion
[[apps]]
name               = "git checkout"
path               = "git"
args               = ["checkout"]
completion         = "command"
completion_command = "git branch --format='%(refname:short)'"
workdir            = "~/src/myproject"

# Override completion settings for scan_dirs items
[[overrides]]
name               = "scoop"
completion         = "list"
completion_list    = ["install", "uninstall", "update", "search", "info"]

# Auto-register scripts from a directory
[[scan_dirs]]
path       = "~/.local/bin"
recursive  = false
extensions = ["sh", "py", "ps1", "cmd"]
```

### Template placeholders in `path` and `args`

You can use [Tera](https://keats.github.io/tera/) template syntax in the `path` and `args` fields of `[[apps]]` entries. Templates are evaluated at launch time when extra args are provided via Args mode (`Tab`).

**Context variables:**

| Variable | Value |
|---|---|
| `{{ args }}` | Extra args joined by space (raw) |
| `{{ args_list }}` | Extra args as an array |
| `{{ env.VAR_NAME }}` | Environment variable |
| `{{ vars.my_var }}` | User-defined variable from `[vars]` in `config.toml` |

**Any Tera filter can be applied**, e.g.:

| Expression | Result |
|---|---|
| `{{ args \| urlencode }}` | URL-encoded (spaces → `%20`) |
| `{{ args \| upper }}` | UPPERCASED |
| `{{ args_list \| join(sep=",") }}` | Joined with custom separator |

**Example workflow** — search Google:

1. Register in `config.toml`:
   ```toml
   [[apps]]
   name = "Google"
   path = "https://www.google.com/search?q={{ args | urlencode }}"
   ```
2. Open shun → type `goo` → press `Tab` → type `rust borrow checker` → `Enter`
3. Opens `https://www.google.com/search?q=rust%20borrow%20checker`

The search query is also saved to history, so next time typing `goo` will show `Google › rust borrow checker` as a candidate.

**User-defined variables** — define reusable values in `[vars]` and reference them with `{{ vars.name }}`:

```toml
[vars]
src_dir  = "~/src/github.com/yourname"
work_dir = "C:/work/projects"

[[apps]]
name       = "Open Project"
path       = "neovide"
args       = ["{{ vars.src_dir }}/{{ args }}"]
completion = "path"

[[apps]]
name = "Work File"
path = "neovide"
args = ["{{ vars.work_dir }}/{{ args }}"]
```

Machine-specific values can go in `config.local.toml` — local `[vars]` entries override or extend the base config.

**Environment variables** — use `{{ env.VAR_NAME }}` or the Tera built-in `get_env()`:

```toml
[[apps]]
name = "Open Project"
path = "neovide"
args = ["{{ env.USERPROFILE }}/src/{{ args }}"]

[[apps]]
name = "Work Dir"
path = '{{ get_env(name="WORK_DIR", default="~/work") }}/{{ args }}'
```

## Theming

Set a built-in theme or override individual colors in `config.toml`:

```toml
[theme]
preset = "nord"    # pick a preset
bg     = "#1a1a2e" # optional: override any individual color
```

**Built-in presets:**

| Preset | Description |
|---|---|
| `catppuccin-mocha` | Default — dark, muted pastel |
| `catppuccin-latte` | Light variant of Catppuccin |
| `nord` | Arctic, blue-toned dark |
| `dracula` | Dark with vibrant purple/pink |
| `tokyo-night` | Dark blue-purple, Tokyo Night style |

**Color variables:**

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

All color keys are optional — unset keys fall back to the chosen `preset`.

## Keybindings

All keybindings are configurable via `[keybindings]` in `config.toml`.

**Key name reference:**
- In-app keybindings (`next`, `confirm`, `close`, etc.) use [KeyboardEvent.key values](https://developer.mozilla.org/en-US/docs/Web/API/UI_Events/Keyboard_event_key_values)
- The global `launch` shortcut uses [global-hotkey key codes](https://docs.rs/global-hotkey/latest/global_hotkey/hotkey/enum.Code.html)

### Search mode

| Key | Action |
|---|---|
| `Alt+Space` | Show / hide launcher |
| `Ctrl+n` / `↓` | Next item |
| `Ctrl+p` / `↑` | Previous item |
| `Enter` | Launch selected item (history-first) |
| `Shift+Enter` | Launch typed query as base item (skip history results) |
| `Tab` | Enter args mode / apply path completion |
| `Ctrl+f` | Accept next word/segment of ghost text |
| `Ctrl+e` | Accept full ghost text |
| `Escape` | Hide launcher |

### Args mode

| Key | Action |
|---|---|
| `Enter` | Launch with args (file completion → launch immediately) |
| `Tab` | Apply selected completion |
| `Ctrl+n` / `Ctrl+p` | Navigate completion list |
| `Ctrl+f` | Accept next word/segment of ghost text |
| `Ctrl+e` | Accept full ghost text |
| `Ctrl+w` | Delete word before cursor |
| `Ctrl+u` | Delete to beginning of line |
| `Escape` | Back to search |

### History management

| Key | Action |
|---|---|
| `Ctrl+d` | Delete selected **History** item from history |

### Slash commands

| Command | Action |
|---|---|
| `/exit` | Quit shun |
| `/config` | Open config file in default editor |
| `/history` | Open history file in default editor |
| `/rescan` | Rescan apps and directories |
| `/update` | Install latest release (shows version if update available) |
| `/version` | Show current version |
| `/theme <name>` | Switch theme instantly and save to `config.local.toml` |

## Special input

| Input | Action |
|---|---|
| `https://...` | Open URL in default browser (ghost text + history) |
| `~/...`, `C:/...` | Browse filesystem, open in file manager |
| `/exit`, `/config`, `/rescan` | Run built-in slash command |

## Building from source

```bash
# Prerequisites: Node.js, Rust

git clone https://github.com/yukimemi/shun
cd shun
npm install
npm run tauri dev     # development
npm run tauri build   # production build
```

## License

MIT
