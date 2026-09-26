#!/bin/sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
states='01 02 04 05 06 08 09 10 11 12 13 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 44 45 46 47 48 49 50 51 53 54 55 56 60 66 69 72 78'

die() { echo "$*" >&2; exit 1; }
check_year() { case "$1" in 201[0-9]|202[0-5]) ;; *) die 'vintage must be 2010 through 2025';; esac; }
filename() { if [ "$1" = 2010 ]; then echo "tl_${1}_${2}_tract10.zip"; else echo "tl_${1}_${2}_tract.zip"; fi; }
source_url() {
  if [ "$1" = 2010 ]; then echo "https://www2.census.gov/geo/tiger/TIGER2010/TRACT/2010/$(filename "$1" "$2")";
  else echo "https://www2.census.gov/geo/tiger/TIGER${1}/TRACT/$(filename "$1" "$2")"; fi
}
sha256() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d ' ' -f 1;
  else sha256sum "$1" | cut -d ' ' -f 1; fi
}
bytes() { wc -c < "$1" | tr -d ' '; }
valid_zip() { [ -s "$1" ] && unzip -tq "$1" >/dev/null 2>&1 && unzip -Z -1 "$1" | grep -Fx "${2%.zip}.shp" >/dev/null; }
download_archive() {
  url=$1 path=$2 name=$3
  part="$path.part"
  [ ! -e "$part" ] || die "inspect existing staging file: $part"
  curl --fail --location --retry 8 --retry-delay 5 --output "$part" "$url" || die "download failed: $url"
  valid_zip "$part" "$name" || die "invalid source archive: $url"
  mv "$part" "$path"
}
check_lock() {
  year=$1 lock="scripts/sources-$1.lock.json"
  [ -f "$lock" ] || die "missing source lock: $lock"
  [ "$(jq 'length' "$lock")" = 56 ] || die "source lock must have 56 records: $lock"
  i=0
  for state in $states; do
    expected_name=$(filename "$year" "$state")
    expected_url=$(source_url "$year" "$state")
    jq -e --argjson i "$i" --argjson year "$year" --arg state "$state" --arg name "$expected_name" --arg url "$expected_url" \
      '.[$i] | .year == $year and .state == $state and .filename == $name and .url == $url and (.bytes | type == "number" and . > 1000) and (.sha256 | test("^[0-9a-f]{64}$"))' "$lock" >/dev/null || die "bad source lock record $i: $lock"
    i=$((i + 1))
  done
}
verify_source() {
  year=$1 state=$2 sources=$3
  name=$(filename "$year" "$state") path="$sources/$(filename "$year" "$state")"
  [ -f "$path" ] || die "missing source: $path"
  expected_bytes=$(jq -r --arg state "$state" '.[] | select(.state == $state) | .bytes' "scripts/sources-$year.lock.json")
  expected_hash=$(jq -r --arg state "$state" '.[] | select(.state == $state) | .sha256' "scripts/sources-$year.lock.json")
  [ "$(bytes "$path")" = "$expected_bytes" ] && [ "$(sha256 "$path")" = "$expected_hash" ] && valid_zip "$path" "$name" || die "source differs from lock: $path"
}
