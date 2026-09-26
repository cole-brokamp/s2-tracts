#!/bin/sh
set -eu
. "$(dirname "$0")/common.sh"
[ "$#" -eq 2 ] && [ "$1" = --vintage ] || die 'usage: scripts/prepare.sh --vintage YEAR'
check_year "$2"
year=$2
check_lock "$year"
for state in $states; do verify_source "$year" "$state" work/sources; done
cargo build --offline --locked --release --features data-prep --bin prepare-tracts
target/release/prepare-tracts --vintage "$year" --sources work/sources --output "dist/census-tracts-$year-r2"
