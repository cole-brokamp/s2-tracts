#!/bin/sh
set -eu

repository='cole-brokamp/s2-tracts'
if [ "$#" -ne 1 ] || ! printf '%s\n' "$1" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo 'usage: sh install.sh vMAJOR.MINOR.PATCH' >&2
  exit 1
fi
version=$1
base="https://github.com/$repository/releases/download/$version"
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
installed_version="$("$tmp_dir/$asset" --version)" || {
  echo 'Downloaded binary cannot run on this system' >&2
  exit 1
}
if [ "$installed_version" != "s2-tracts ${version#v}" ]; then
  echo "Downloaded binary version does not match $version" >&2
  exit 1
fi
"$tmp_dir/$asset" data install --vintage 2020
mkdir -p "$bin_dir"
mv "$tmp_dir/$asset" "$bin_dir/s2-tracts"
printf 'Installed %s with 2020 tract data\n' "$bin_dir/s2-tracts"
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) printf 'Add %s to your PATH to run s2-tracts directly.\n' "$bin_dir" ;;
esac
