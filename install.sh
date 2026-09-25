#!/bin/sh
set -eu

repository='cole-brokamp/s2-tracts'
base="https://github.com/$repository/releases/latest/download"
case "$(uname -s)" in
  Darwin) os=macos ;;
  Linux) os=linux ;;
  *) echo 's2-tracts supports macOS and Linux' >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  arm64|aarch64) arch=aarch64 ;;
  *) echo 'unsupported CPU architecture' >&2; exit 1 ;;
esac
asset="s2-tracts-$os-$arch"
bin_dir="${S2_TRACTS_BIN_DIR:-$HOME/.local/bin}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

curl --fail --location --retry 3 --output "$tmp_dir/$asset" "$base/$asset"
curl --fail --location --retry 3 --output "$tmp_dir/$asset.sha256" "$base/$asset.sha256"
expected="$(cut -d ' ' -f 1 < "$tmp_dir/$asset.sha256")"
if command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp_dir/$asset" | cut -d ' ' -f 1)"
elif command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp_dir/$asset" | cut -d ' ' -f 1)"
else
  echo 'SHA-256 verification requires shasum or sha256sum' >&2
  exit 1
fi
if [ "$actual" != "$expected" ]; then
  echo 'binary SHA-256 mismatch' >&2
  exit 1
fi
chmod 755 "$tmp_dir/$asset"
mkdir -p "$bin_dir"
mv "$tmp_dir/$asset" "$bin_dir/s2-tracts"
printf 'Installed %s\n' "$bin_dir/s2-tracts"
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) printf 'Add %s to your PATH to run s2-tracts directly.\n' "$bin_dir" ;;
esac
printf 'Data downloads on first lookup. To prefetch a year, run: %s data install --vintage 2020\n' "$bin_dir/s2-tracts"
if [ -r /dev/tty ] && [ -w /dev/tty ]; then
  printf 'Install the default 2020 tract data now? [y/N] ' > /dev/tty
  if IFS= read -r answer < /dev/tty; then
    case "$answer" in
      y|Y|yes|YES) "$bin_dir/s2-tracts" data install --vintage 2020 ;;
    esac
  fi
fi
