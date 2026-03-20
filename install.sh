#!/bin/sh
set -e

repo="yukimemi/shun"

echo "Fetching latest release..."
release_json=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest")
version=$(echo "$release_json" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
version_no_v="${version#v}"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Darwin)
    url="https://github.com/$repo/releases/download/$version/shun_${version_no_v}_universal.dmg"
    tmp=$(mktemp -d)
    echo "Downloading shun $version..."
    curl -fsSL -o "$tmp/shun.dmg" "$url"
    echo "Mounting DMG..."
    hdiutil attach "$tmp/shun.dmg" -mountpoint "$tmp/mnt" -quiet -nobrowse
    install_dir="$HOME/Applications"
    mkdir -p "$install_dir"
    echo "Installing to $install_dir ..."
    cp -r "$tmp/mnt/shun.app" "$install_dir/"
    hdiutil detach "$tmp/mnt" -quiet
    rm -rf "$tmp"
    echo ""
    echo "shun $version installed to $install_dir/shun.app"
    echo "Open it from Finder or run: open ~/Applications/shun.app"
    ;;

  Linux)
    case "$arch" in
      x86_64)
        url="https://github.com/$repo/releases/download/$version/shun_${version_no_v}_amd64.AppImage"
        dest="$HOME/.local/bin/shun"
        mkdir -p "$HOME/.local/bin"
        echo "Downloading shun $version..."
        curl -fsSL -o "$dest" "$url"
        chmod +x "$dest"
        echo ""
        echo "shun $version installed to $dest"
        case ":$PATH:" in
          *":$HOME/.local/bin:"*) ;;
          *) echo "Note: Add ~/.local/bin to your PATH to run shun from the terminal." ;;
        esac
        ;;
      *)
        echo "Unsupported architecture: $arch"
        echo "Download manually from: https://github.com/$repo/releases"
        exit 1
        ;;
    esac
    ;;

  *)
    echo "Unsupported OS: $os"
    echo "Download manually from: https://github.com/$repo/releases"
    exit 1
    ;;
esac
