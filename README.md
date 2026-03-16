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

## Features

- **Instant popup** — global hotkey brings up the launcher anywhere
- **Fuzzy / exact search** — powered by [nucleo-matcher](https://github.com/helix-editor/nucleo) (the same engine as Helix editor)
- **Launch history** — frecency sorting (count-first or recent-first)
- **Args mode** — press `Tab` to pass extra arguments to any app
- **Path completion** — ghost text + dropdown with `Ctrl-n/p`, `Ctrl-f/e`
- **Configurable completion** — `path` / `list` / `command` / `none` per app
- **URL & path navigation** — type `https://...` or `~/...` to open directly
- **Slash commands** — `/exit`, `/config`
- **Multi-monitor** — appears on the monitor where your cursor is
- **Minimal UI** — borderless, transparent, always-on-top
- **Cross-platform** — Windows, macOS, Linux

## Installation

Download the latest installer from [Releases](https://github.com/yukimemi/shun/releases).

| Platform | File |
|---|---|
| Windows | `.msi` (recommended) or `.exe` |
| macOS | `.dmg` (universal — Apple Silicon + Intel) |
| Linux | `.AppImage` or `.deb` |

## Configuration

Config file is created automatically on first launch:

| Platform | Path |
|---|---|
| Windows | `%APPDATA%\shun\config.toml` |
| macOS | `~/Library/Application Support/shun/config.toml` |
| Linux | `~/.config/shun/config.toml` |

### Example `config.toml`

```toml
# Search mode: "fuzzy" (default) | "exact"
search_mode = "fuzzy"

# Sort order: "count_first" (default) | "recent_first"
sort_order = "recent_first"

[keybindings]
launch      = "Alt+Space"   # Global hotkey to show/hide
next        = "Ctrl+n"
prev        = "Ctrl+p"
confirm     = "Enter"
arg_mode    = "Tab"
word_accept = "Ctrl+f"
line_accept = "Ctrl+e"
close       = "Escape"

# Open editor with file path completion
[[apps]]
name             = "Neovide"
path             = "neovide"
allow_extra_args = true
completion       = "path"       # "path" | "none" | "list" | "command"

# Open a URL directly
[[apps]]
name = "GitHub"
path = "https://github.com"

# systemctl with fixed subcommand list
[[apps]]
name            = "systemctl"
path            = "systemctl"
allow_extra_args = true
completion      = "list"
completion_list = ["start", "stop", "restart", "status", "enable", "disable", "reload"]

# docker exec into a running container (completion from docker ps)
[[apps]]
name               = "docker exec"
path               = "docker"
args               = ["exec", "-it"]
allow_extra_args   = true
completion         = "command"
completion_command = "docker ps --format '{{.Names}}'"

# git checkout with branch name completion
[[apps]]
name               = "git checkout"
path               = "git"
args               = ["checkout"]
allow_extra_args   = true
completion         = "command"
completion_command = "git branch --format='%(refname:short)'"
workdir            = "~/src/myproject"

# Auto-register scripts from a directory
[[scan_dirs]]
path       = "~/.local/bin"
recursive  = false
extensions = ["sh", "py", "ps1", "cmd"]
```

## Keybindings

### Search mode

| Key | Action |
|---|---|
| `Alt+Space` | Show / hide launcher |
| `Ctrl+n` / `↓` | Next item |
| `Ctrl+p` / `↑` | Previous item |
| `Enter` | Launch selected item |
| `Tab` | Enter args mode (if `allow_extra_args = true`) |
| `Escape` | Hide launcher |

### Args mode

| Key | Action |
|---|---|
| `Enter` | Launch with args (file completion → launch immediately) |
| `Tab` | Apply selected completion |
| `Ctrl+n` / `Ctrl+p` | Navigate completion list |
| `Ctrl+f` | Accept next path segment (ghost text) |
| `Ctrl+e` | Accept full completion (ghost text) |
| `Escape` | Back to search |

### Path navigation (type `~/` or `C:/`)

| Key | Action |
|---|---|
| `Tab` | Apply selected completion |
| `Ctrl+f` | Accept next path segment |
| `Ctrl+e` | Accept full path |
| `Enter` | Open in file manager |

### Slash commands

| Command | Action |
|---|---|
| `/exit` | Quit shun |
| `/config` | Open config file in default editor |

## Special input

| Input | Action |
|---|---|
| `https://...` | Open URL in default browser |
| `~/...`, `C:/...` | Browse filesystem, open in file manager |
| `/exit`, `/config` | Run built-in slash command |

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
