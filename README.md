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
- **Local config override** — `config.local.toml` merges machine-specific settings without touching the main config (chezmoi-friendly)
- **Auto-hide on blur** — optionally hide when focus leaves the launcher
- **Multi-monitor** — appears on the monitor where your cursor is
- **Minimal UI** — borderless, transparent, always-on-top
- **Cross-platform** — Windows, macOS, Linux

## Installation

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
| `search_mode`, `sort_order`, `hide_on_blur`, `update_check_interval` | Local value **overrides** main (only when explicitly set) |
| `[keybindings]` | **Per-field override** — only specified keys are overridden |

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

[keybindings]
launch      = "Alt+Space"   # Global hotkey to show/hide
next        = "Ctrl+n"
prev        = "Ctrl+p"
confirm     = "Enter"
arg_mode    = "Tab"
accept_word = "Ctrl+f"      # Accept next word/segment of ghost text
accept_line = "Ctrl+e"      # Accept full ghost text
delete_word = "Ctrl+w"      # Delete word before cursor (args mode)
delete_line = "Ctrl+u"      # Delete to beginning of line (args mode)
run_query   = "Shift+Enter" # Run typed query directly (skip history results)
close       = "Escape"
delete_item = "Ctrl+d"     # Delete selected history item



# Open editor with file path completion
[[apps]]
name       = "Neovide"
path       = "neovide"
completion = "path"       # "path" | "none" | "list" | "command"

# Open a URL directly
[[apps]]
name = "GitHub"
path = "https://github.com"

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
| `/rescan` | Rescan apps and directories |
| `/update` | Install latest release (shows version if update available) |
| `/version` | Show current version |
| `/history` | Open history file in default editor |

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
