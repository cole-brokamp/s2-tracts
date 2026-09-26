#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
  echo 'usage: scripts/release-data.sh YEAR TAG' >&2
  exit 2
fi
year=$1
tag=$2
case "$year" in
  201[0-9]|202[0-5]) ;;
  *) echo 'YEAR must be 2010 through 2025' >&2; exit 2 ;;
esac
file="dist/census-tracts-$year-r2/tracts_$year.fgb"
test -f "$file" || { echo "missing $file" >&2; exit 1; }
archive="$file.zst"
test -f "$archive" || { echo "missing $archive; run sh scripts/assets.sh freeze" >&2; exit 1; }
sh scripts/assets.sh verify "$year" "$file" "$archive"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM
provenance="$tmp_dir/provenance_$year.json"
cp "$(dirname "$file")/provenance.json" "$provenance"
gh release upload "$tag" "$archive" "$file.sha256" "$archive.sha256" "$provenance" --repo cole-brokamp/s2-tracts --clobber
