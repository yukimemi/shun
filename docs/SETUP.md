# Package Manager Registration Setup

This document describes how to configure the external repositories and GitHub secrets
required for automatic Scoop, Homebrew, and WinGet publishing on each release.

---

## 1. Scoop Bucket (`yukimemi/scoop-bucket`)

The release workflow updates `bucket/shun.json` in a separate GitHub repository named
`yukimemi/scoop-bucket`.

### Repository structure

```
yukimemi/scoop-bucket/
└── bucket/
    └── shun.json    ← copy from this repo's bucket/shun.json
```

### Setup steps

1. Create a new GitHub repository named `scoop-bucket` under the `yukimemi` account.
2. Create the `bucket/` directory and add `shun.json` (initial content is in `bucket/shun.json` of this repo).
3. Create a GitHub Personal Access Token (PAT) with `Contents: Read and write` permission scoped to the `yukimemi/scoop-bucket` repository.
4. Add the PAT as a secret named `SCOOP_BUCKET_PAT` in the `yukimemi/shun` repository settings.

### User installation

```powershell
# Add the bucket
scoop bucket add yukimemi https://github.com/yukimemi/scoop-bucket

# Install shun
scoop install yukimemi/shun
```

---

## 2. Homebrew Tap (`yukimemi/homebrew-tap`)

The release workflow updates `Casks/shun.rb` in a separate GitHub repository named
`yukimemi/homebrew-tap`.

### Repository structure

```
yukimemi/homebrew-tap/
└── Casks/
    └── shun.rb    ← copy from this repo's homebrew/shun.rb
```

### Setup steps

1. Create a new GitHub repository named `homebrew-tap` under the `yukimemi` account.
2. Create the `Casks/` directory and add `shun.rb` (initial content is in `homebrew/shun.rb` of this repo).
3. Create a GitHub Personal Access Token (PAT) with `Contents: Read and write` permission scoped to the `yukimemi/homebrew-tap` repository.
4. Add the PAT as a secret named `HOMEBREW_TAP_PAT` in the `yukimemi/shun` repository settings.

### User installation

```bash
# Add the tap
brew tap yukimemi/tap

# Install shun
brew install --cask yukimemi/tap/shun
```

---

## 3. WinGet (`winget-pkgs`)

The release workflow uses [`vedantmgoyal9/winget-releaser`](https://github.com/vedantmgoyal9/winget-releaser)
to automatically submit a PR to the official [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs)
repository.

### Setup steps

1. Fork `microsoft/winget-pkgs` (the action requires a fork to open PRs).
2. Follow the [winget-releaser setup guide](https://github.com/vedantmgoyal9/winget-releaser#setup) to
   generate a token that has access to your fork.
3. Add the token as a secret named `WINGET_TOKEN` in the `yukimemi/shun` repository settings.

The initial manifests in `winget/` are reference copies. The actual submission to
`microsoft/winget-pkgs` is handled automatically by the `submit-winget` job on each release.

### User installation

```powershell
# Install shun via WinGet
winget install yukimemi.shun
```

---

## 4. Auto-tag on version bump (`RELEASE_TAG_TOKEN`)

`.github/workflows/auto-tag.yml` watches pushes to `main` and, whenever a commit changes the
`version` field in `src-tauri/Cargo.toml`, creates and pushes a matching `vX.Y.Z` tag. That tag
push is what triggers `.github/workflows/release.yml` (the build/sign/publish workflow), so this
is what turns "a version-bump PR merged to main" into "a release goes out" without anyone running
`git tag` by hand.

The tag must be pushed with a personal access token (PAT), not the default `GITHUB_TOKEN`: GitHub
deliberately does not fire downstream workflows (`release.yml`) for refs pushed by the default
token, to prevent recursive workflow runs.

### Setup steps

1. Create a GitHub Personal Access Token (PAT) with `Contents: Read and write` permission scoped
   to the `yukimemi/shun` repository.
2. Add the PAT as a secret named `RELEASE_TAG_TOKEN` in the `yukimemi/shun` repository settings.

### Unlike the other secrets on this page, this one is not optional

If `RELEASE_TAG_TOKEN` is missing, `auto-tag.yml` does **not** skip quietly — it fails the job
with an `::error::` annotation as soon as it detects a version bump that it can't tag. This is
intentional: a silent skip here reproduces the exact "PR merged, no release ever showed up"
problem the workflow exists to prevent. Watch the Actions tab (or set up notifications) after
merging a version-bump PR until this secret is confirmed working.

See `CLAUDE.md`'s "Tagging a release" section for the day-to-day release flow that depends on
this secret.

---

## GitHub Secrets Summary

Add the following secrets in `yukimemi/shun` → Settings → Secrets and variables → Actions:

| Secret name          | Description                                                         | Required for    |
|-----------------------|---------------------------------------------------------------------|-----------------|
| `RELEASE_TAG_TOKEN`  | PAT with write access to `yukimemi/shun`, used to push release tags | Auto-tag → release trigger (**required** — job fails loudly if unset) |
| `SCOOP_BUCKET_PAT`  | PAT with write access to `yukimemi/scoop-bucket`                    | Scoop auto-update |
| `HOMEBREW_TAP_PAT`  | PAT with write access to `yukimemi/homebrew-tap`                    | Homebrew auto-update |
| `WINGET_TOKEN`      | Token for `vedantmgoyal9/winget-releaser` to submit WinGet PRs      | WinGet submission |

`RELEASE_TAG_TOKEN` is required — see section 4 above. For the other three, if a secret is not
set, the corresponding job step is skipped gracefully — no build failure occurs.

---

## How it works

On each release tag (`v*.*.*`), after the `release` job completes:

1. **`update-scoop`** (ubuntu-latest): Downloads `shun-windows-x64.zip`, computes SHA256,
   updates `bucket/shun.json` in `yukimemi/scoop-bucket`, and pushes the change.

2. **`update-homebrew`** (macos-latest): Downloads `shun_VERSION_universal.dmg`, computes SHA256,
   updates `Casks/shun.rb` in `yukimemi/homebrew-tap`, and pushes the change.

3. **`submit-winget`** (windows-latest): Uses `vedantmgoyal9/winget-releaser` to detect the new
   `.msi` release asset and open a PR against `microsoft/winget-pkgs`.
