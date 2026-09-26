#!/bin/sh
set -eu
. "$(dirname "$0")/common.sh"
manifest=scripts/assets.lock.json

verify_raw() {
  year=$1 file=$2
  [ -f "$file" ] || die "missing prepared file: $file"
  provenance="$(dirname "$file")/provenance.json"
  [ -f "$provenance" ] || die "missing provenance: $provenance"
  raw_bytes=$(bytes "$file") raw_hash=$(sha256 "$file")
  jq -e --argjson year "$year" --argjson bytes "$raw_bytes" --arg hash "$raw_hash" \
    '.bundle == ("census-tracts-" + ($year|tostring) + "-r2") and
     (.years | length == 1) and .years[0].year == $year and
     .years[0].bytes == $bytes and .years[0].sha256 == $hash and
     (.years[0].sources | length == 56)' "$provenance" >/dev/null || die "prepared file differs from provenance: $file"
}

verify_archive() {
  archive=$1
  [ -f "$archive" ] && zstd --test --quiet "$archive" || die "invalid compressed file: $archive"
  archive_bytes=$(bytes "$archive") archive_hash=$(sha256 "$archive")
}

case "${1-}" in
  raw)
    [ "$#" -eq 2 ] || die 'usage: scripts/assets.sh raw YEAR'
    check_year "$2"
    verify_raw "$2" "dist/census-tracts-$2-r2/tracts_$2.fgb"
    ;;
  freeze)
    [ "$#" -eq 1 ] || { [ "$#" -eq 2 ] && [ "$2" = --replace ]; } || die 'usage: scripts/assets.sh freeze [--replace]'
    [ ! -e "$manifest.building" ] || die "inspect existing staging file: $manifest.building"
    records=$(mktemp)
    trap 'rm -f "$records"' EXIT HUP INT TERM
    for year in $(seq 2010 2025); do
      file="dist/census-tracts-$year-r2/tracts_$year.fgb"
      verify_raw "$year" "$file"
      archive="$file.zst"
      if [ ! -e "$archive" ]; then
        [ ! -e "$archive.building" ] || die "inspect existing staging file: $archive.building"
        zstd -9 --quiet "$file" -o "$archive.building" && mv "$archive.building" "$archive"
      fi
      verify_archive "$archive"
      jq -cn --arg year "$year" --argjson raw_bytes "$raw_bytes" --arg raw_sha256 "$raw_hash" \
        --argjson archive_bytes "$archive_bytes" --arg archive_sha256 "$archive_hash" \
        '{year:$year,record:{raw_bytes:$raw_bytes,raw_sha256:$raw_sha256,archive_bytes:$archive_bytes,archive_sha256:$archive_sha256}}' >> "$records"
      echo "$year $raw_bytes $archive_bytes" >&2
    done
    jq -s 'map({key:.year,value:.record}) | from_entries' "$records" > "$manifest.building"
    if [ -f "$manifest" ]; then
      if jq -e --slurpfile actual "$manifest.building" '. == $actual[0]' "$manifest" >/dev/null; then
        rm "$manifest.building"
        exit 0
      fi
      [ "$#" -eq 2 ] || die "asset manifest differs; inspect before replacing: $manifest.building"
    fi
    mv "$manifest.building" "$manifest"
    ;;
  verify)
    [ "$#" -eq 4 ] || die 'usage: scripts/assets.sh verify YEAR FILE ARCHIVE'
    check_year "$2"
    year=$2 file=$3 archive=$4
    [ "$(basename "$file")" = "tracts_$year.fgb" ] && [ "$archive" = "$file.zst" ] || die 'unexpected asset names'
    verify_raw "$year" "$file"
    verify_archive "$archive"
    jq -e --arg year "$year" --argjson raw_bytes "$raw_bytes" --arg raw_sha256 "$raw_hash" \
      --argjson archive_bytes "$archive_bytes" --arg archive_sha256 "$archive_hash" \
      '.[$year] == {raw_bytes:$raw_bytes,raw_sha256:$raw_sha256,archive_bytes:$archive_bytes,archive_sha256:$archive_sha256}' "$manifest" >/dev/null || die "release asset differs from pinned manifest: $file"
    printf '%s  %s\n' "$raw_hash" "$(basename "$file")" > "$file.sha256"
    printf '%s  %s\n' "$archive_hash" "$(basename "$archive")" > "$archive.sha256"
    ;;
  *) die 'usage: scripts/assets.sh raw YEAR | freeze [--replace] | verify YEAR FILE ARCHIVE' ;;
esac
