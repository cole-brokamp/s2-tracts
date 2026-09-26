#!/bin/sh
set -eu
. "$(dirname "$0")/common.sh"
[ "$#" -eq 2 ] && [ "$1" = --vintage ] || die 'usage: scripts/lock-year.sh --vintage YEAR'
check_year "$2"
year=$2 lock="scripts/sources-$2.lock.json" sources=work/sources
[ ! -e "$lock" ] && [ ! -e "$lock.building" ] || die "refusing to overwrite: $lock"
mkdir -p "$sources"
records=$(mktemp)
trap 'rm -f "$records"' EXIT HUP INT TERM
for state in $states; do
  name=$(filename "$year" "$state") url=$(source_url "$year" "$state") path="$sources/$name"
  if ! valid_zip "$path" "$name"; then
    [ ! -e "$path" ] || die "invalid cached source; inspect: $path"
    download_archive "$url" "$path" "$name"
  fi
  jq -cn --argjson year "$year" --arg state "$state" --arg url "$url" --arg filename "$name" \
    --argjson bytes "$(bytes "$path")" --arg sha256 "$(sha256 "$path")" \
    '{year:$year,state:$state,url:$url,filename:$filename,bytes:$bytes,sha256:$sha256}' >> "$records"
  echo "$year $state: locked" >&2
done
jq -s . "$records" > "$lock.building"
mv "$lock.building" "$lock"
