#!/usr/bin/env bash
# Wraps the real /usr/bin/codesign so tauri-action's macOS ad-hoc signing
# (`--sign -` / `-s -`, used because this repo has no Apple Developer ID
# certificate) gets a Designated Requirement pinned to a fixed identifier
# instead of the default one, which pins a content hash (cdhash).
#
# Why this matters: a cdhash-pinned DR changes on every rebuild, so macOS's
# TCC (Accessibility permission, used by app_window.rs's window_title feature
# on macOS) treats every new shun release as a different app — the user has
# to re-grant Accessibility after every update, and stale rows pile up in
# System Settings > Privacy & Security > Accessibility. Confirmed by
# yukimemi/kagi's own investigation (src/platform/macos.rs there) and
# reproduced independently here: re-signing with an identifier-only DR fixes
# it, no certificate needed (see PR description / app_window.rs macos_impl
# doc for the full writeup).
#
# This script is installed earlier in PATH than /usr/bin/codesign for the
# duration of the macOS release job only (see release.yml), so every
# `codesign --sign -` call tauri-action makes internally is intercepted
# transparently — no changes to tauri.conf.json or tauri-action's own
# behavior needed, and no certificate/secret management.
set -euo pipefail

readonly REAL_CODESIGN=/usr/bin/codesign
readonly SIGN_ID=com.yukimemi.shun

args=("$@")
is_adhoc=0
has_requirement=0

for i in "${!args[@]}"; do
  case "${args[$i]}" in
    --sign | -s)
      [[ "${args[$((i + 1))]:-}" == "-" ]] && is_adhoc=1
      ;;
    -r | -r=* | --requirements)
      has_requirement=1
      ;;
  esac
done

if [[ "$is_adhoc" == "1" && "$has_requirement" == "0" ]]; then
  args+=(--identifier "$SIGN_ID" "-r=designated => identifier \"$SIGN_ID\"")
fi

exec "$REAL_CODESIGN" "${args[@]}"
