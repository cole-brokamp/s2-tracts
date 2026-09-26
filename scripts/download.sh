#!/bin/sh
set -eu
. "$(dirname "$0")/common.sh"
[ "$#" -eq 2 ] && [ "$1" = --vintage ] || die 'usage: scripts/download.sh --vintage YEAR'
check_year "$2"
year=$2 sources=work/sources
check_lock "$year"
mkdir -p "$sources"
for state in $states; do
  name=$(filename "$year" "$state") path="$sources/$name"
  if [ ! -e "$path" ]; then download_archive "$(source_url "$year" "$state")" "$path" "$name"; fi
  verify_source "$year" "$state" "$sources"
  echo "$year $state: verified" >&2
done
