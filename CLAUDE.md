# CLAUDE.md — shun (瞬)

Project conventions and guidelines for Claude Code.

## Project overview

**shun** is a cross-platform keyboard-driven minimal launcher (like Alfred/Raycast) built with:
- **Backend**: Rust + Tauri v2
- **Frontend**: Svelte 5 (runes: `$state`, `$derived`, `$effect`) + TypeScript
- **Search**: nucleo-matcher (fuzzy/exact)

## Repository layout

```
src/                     # SvelteKit frontend
  routes/+page.svelte    # Single-page UI (all launcher logic lives here)
  lib/utils.js           # Pure frontend helpers: firstSepIdx, isPathQuery, matchKey
  lib/utils.test.js      # Vitest tests for utils
src-tauri/
  src/
    lib.rs               # Tauri app entrypoint, all #[tauri::command]s, plugin setup
    config.rs            # Config struct, TOML parsing, defaults
    apps.rs              # App discovery, LaunchItem, launch_with_extra
    search.rs            # fuzzy/exact filter via nucleo-matcher
    complete.rs          # Path/list/command completion
    history.rs           # Frecency history (count + last_used)
    utils.rs             # expand_path
  Cargo.toml
  tauri.conf.json        # Tauri config including updater pubkey
.github/workflows/
  ci.yml                 # PR/push to main: frontend + Rust tests
  release.yml            # Tag v*.*.*: tests → build → sign → publish
```

## Commands

```bash
cargo make setup     # one-time on clone: pre-push hook + APM install
cargo make check     # fmt + clippy + Rust tests + frontend tests
cargo make fmt       # apply rustfmt
cargo make tauri-dev # npm run tauri dev (shortcut)

# Direct (without cargo-make):
npm run dev          # Start dev server (Tauri hot-reload)
npm run tauri dev    # Launch full Tauri app in dev mode
npm run build        # Build frontend
npm run tauri build  # Production build
npm test             # Vitest (frontend unit tests)
cd src-tauri && cargo test   # Rust unit tests
```

`cargo make setup` is `hook-install` + `apm-install`:

- `hook-install` wires `.git/hooks/pre-push` to `cargo make check`.
- `apm-install` requires the
  [APM](https://github.com/microsoft/apm) CLI on `PATH`
  (`scoop install apm` on Windows, `brew install microsoft/apm/apm`
  on macOS, `pip install apm-cli`, or
  `curl -sSL https://aka.ms/apm-unix | sh`). It runs
  `apm install`, compiling the
  [renri](https://github.com/yukimemi/renri) skill (declared in
  `apm.yml`, pinned to `#main`) into `.claude/skills/` +
  `.gemini/skills/` + `.github/skills/` so AI sessions know how to
  manage worktrees / jj workspaces while developing shun. Lockfile
  is `apm.lock.yaml`. Pinned to `#main`, so `apm install --update`
  always pulls the latest renri skill content.

## Working in this repo with AI agents

- **Read-only inspection** (browsing files, answering questions,
  running read-only commands): no worktree needed; work in the
  existing checkout.
- **Any commit-bound change** — new feature, bug fix, refactor,
  reviewer-feedback fix on an open PR: if you are on the **main
  checkout**, start with `renri add <branch-name>` and move into
  the worktree before committing (`cd "$(renri cd <branch-name>)"`,
  or use the shell wrapper from `renri shell-init` so plain
  `renri cd <name>` cds for you). If you are **already in a
  worktree** (e.g. iterating on an existing PR), keep working
  there. Do **not** edit on the main checkout for non-trivial
  changes.
- **Trivial wording / typo fixes** are the only soft exception, and
  even then `renri add` is cheap enough that defaulting to it is
  fine.

### Backend choice — jj-first

This repo is colocated git+jj. `renri add` defaults to **jj**
(creates a non-colocated jj workspace where `jj` commands work and
`git` does not — see [jj-vcs/jj#8052](https://github.com/jj-vcs/jj/issues/8052)
for why secondary colocation isn't possible yet). Stick to the
default unless there is a specific reason to use git tooling.

```sh
# In a freshly created worktree (default jj backend):
jj st                                               # status
jj describe -m "feat: ..."                          # set @-commit description
jj git push --bookmark <branch-name> --allow-new    # first push of a new branch
jj git push --bookmark <branch-name>                # subsequent pushes
```

`renri --vcs git add <branch-name>` is the override and exists for
genuine git-CLI-only needs (git submodule, native git2 tooling,
git-only hooks). Do **not** reach for it out of git-CLI familiarity
— prefer learning the equivalent jj commands.

### Cleanup after merge

After the PR merges and you've pulled the change into main:

- `renri remove <branch>` — removes a single worktree. Calls
  `git worktree remove` or `jj workspace forget` as appropriate,
  then deletes the directory. Refuses to remove the main worktree.
- `renri prune` — best-effort GC across the repo. Git: removes
  worktree metadata for already-deleted directories. jj: forgets
  workspaces whose root path is gone (the missing
  `jj workspace prune` analog).

Run `renri prune` periodically — especially after manually
`rm -rf`-ing worktree dirs without going through `renri remove`.

### Hooks in worktrees

The pre-push hook installed by `cargo make hook-install` lives in
the **main repo's** `.git/hooks/pre-push`.

- **git worktrees** share that hook directory, so plain `git push`
  from a worktree triggers `cargo make check` automatically.
- **jj workspaces** route their pushes through `jj git push`, which
  uses libgit2 directly and **does not fire git hooks**. From a jj
  workspace, run `cargo make check` manually before
  `jj git push --bookmark <branch-name>` — there is no automatic gate.

### Post-create automation (`cargo make on-add`)

`renri.toml` declares a `[[hooks.post_create]]` that runs
`cargo make on-add` immediately after `renri add` finishes. The
default chain is:

- `apm install --update` — refresh the renri skill so AI agents in
  the new worktree see the latest guidance.
- `vcs-fetch` — `jj git fetch` in a jj workspace, `git fetch`
  otherwise; cleans up subsequent rebase / merge.

Add per-repo extras (e.g. `cargo fetch`, `npm install`) by extending
`[tasks.on-add]`'s dependency list in `Makefile.toml`.

## Key architecture

### State management (Rust)
- `Arc<Mutex<Option<ItemCache>>>` shared across commands via `tauri::State`
- `refresh_cache_bg()` spawns a background thread to rebuild cache
- Cache is refreshed: on startup, on hide, after each launch

### Item sources
- `ItemSource::Apps` — `[[apps]]` entries in config.toml
- `ItemSource::Scan` — discovered from `[[scan_dirs]]`
- `ItemSource::History` — previous launches with extra args (`path\targs` key)
- `ItemSource::Url` — `http://` / `https://` inputs
- `ItemSource::Path` — filesystem paths (`~/...`, `C:/...`)

### Keybindings
- Defined in `config::Keybindings` with serde defaults
- Serialized to frontend via `get_config` command on mount
- Matched in frontend using `matchKey(e, binding)` from `$lib/utils.js`
- Format: `"Ctrl+n"`, `"Alt+Space"`, `"Shift+Enter"`, `"Escape"`

### Ghost text
- Search mode: `searchGhostSuffix` — triggers when `candidate.path.startsWith(query)`
- Args mode: `ghostSuffix` — from `allCompletions[completionIndex]`
- `lastArgsGhost` — first args history entry shown before user types

### Auto-update
- `tauri-plugin-updater` checks GitHub releases on startup (background async)
- Emits `update-available` event with new version string to frontend
- `/update` slash command downloads, installs, and restarts
- Signing key: `~/.tauri/shun.key` (pubkey in `tauri.conf.json`; private key in `TAURI_SIGNING_PRIVATE_KEY` GitHub secret)

## CI/CD

- **CI** (`ci.yml`): runs on push/PR to `main` — frontend tests (`npm test`) + Rust tests (`cargo test`) on ubuntu + windows
- **Release** (`release.yml`): triggered by `v*.*.*` tags — tests must pass before build; `tauri-action` builds, signs, and publishes; generates `latest.json` for auto-update

### Tagging a release
```bash
git tag v1.2.3 && git push origin main && git push origin v1.2.3
```
The release is fully automatic once tests pass.

### Version update in release
Uses `perl -i -pe` (not `sed -i`) — macOS BSD sed doesn't support `-i ''` in the same way.

## Config file

Auto-created at first launch:
- Windows: `%APPDATA%\shun\config.toml`
- macOS: `~/Library/Application Support/shun/config.toml`
- Linux: `~/.config/shun/config.toml`

## Testing

### Rust tests (88 total)
Each module has a `#[cfg(test)]` block:
- `config.rs` — defaults, TOML parsing, keybinding overrides
- `search.rs` — fuzzy/exact/migemo filter
- `complete.rs` — split_last_token, sort_completions, complete_path (uses `tempfile`)
- `history.rs` — sort_key, serde roundtrip, combined key format
- `utils.rs` — expand_path variants
- `apps.rs` — is_url, is_path, launch_with_extra

### Frontend tests (53 total)
`src/lib/utils.test.js` covers `firstSepIdx`, `isPathQuery`, `matchKey`, `shouldBypassTemplate`.

Do not mock these — they are pure functions with no Tauri dependencies.

## Git workflow

- **Never commit directly to `main`** — all code changes must go through a pull request (automated release commits and version bumps by CI are exempt)
- Create a feature branch, commit there, then open a PR with `gh pr create`
- PR titles and bodies must be written in English

## Coding conventions

- Keep all launcher UI logic in `+page.svelte`; extract **pure** functions to `src/lib/utils.js` only when they need to be tested independently (Tauri imports block Vitest)
- Rust: one file per concern; add `#[cfg(test)]` tests in the same file
- Do not use `sed -i` in shell scripts or CI — use `perl -i -pe` for cross-platform compatibility
- `releaseDraft: false` in tauri-action — tests gate the build so no manual publish needed
- README keybindings table and `config.toml` example must be kept in sync with `config.rs` defaults whenever keybindings change
- README slash commands table must be updated whenever a new slash command is added

### Svelte 5 $state pitfall

Only use `$state` for values that are **read reactively** (in templates, `$derived`, or `$effect`). Applying `$state` to a variable that is only written imperatively (internal counters, caches, etc.) will register it as a dependency of any `$effect` that reads it, causing that effect to re-run on every write — creating a reactive loop.

Real example: making `currentWidth` a `$state` caused `resizeForSearch` to track it as a dependency of the resize `$effect`; every `_setSize` call wrote `currentWidth`, re-triggering the effect infinitely.

**AI review tools (coderabbit, etc.) are not always right.** Even a "follow Svelte 5 conventions and use `$state`" suggestion must be verified for side-effects before applying.

## Auto-update notes

- `tauri.conf.json` must have `bundle.createUpdaterArtifacts: true` — without this, tauri-action silently skips `latest.json` with "Signature not found"
- Use `tauri-apps/tauri-action@v0.6` (not `@v0`) — v0.6 properly supports Tauri v2 updater
- Signing key: `~/.tauri/shun.key` (pubkey in `tauri.conf.json`; private key in `TAURI_SIGNING_PRIVATE_KEY` GitHub secret, no password)
- `latest.json` is auto-generated and uploaded by tauri-action to each release

## Current status (2026-03-22)

- Latest tag: **v1.9.1**
- Auto-update fully working: `latest.json` generated, signatures present
- Portable self-update working: `portable.txt` in zip triggers zip-download path
- Download progress display working: `update-progress` events shown in query
- Renovate auto-merge enabled for patch/minor updates
- Version files (package.json, tauri.conf.json, Cargo.toml) auto-committed back to main after each release
- System tray icon with Show / Config / Exit menu
- `[vars]` section in config for user-defined Tera template variables (`{{ vars.my_var }}`)
- Config items use `item.name` as history key (distinguishes apps sharing the same executable)
- Pre-push hook in `.claude/settings.json`: cargo fmt --check, cargo clippy -D warnings, npm test
- Migemo search mode: `rustmigemo` (Rust) + `jsmigemo` (JS); dict bundled as `public/migemo-compact-dict.bin` via `include_bytes!`
- `shouldBypassTemplate` in utils.js: history+template bypass detection
- Rust tests: 88 total / Frontend tests: 53 total
