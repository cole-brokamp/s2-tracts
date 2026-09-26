#!/bin/sh
set -eu

for year in $(seq 2010 2025); do
  output="dist/census-tracts-$year-r2"
  if [ -f "$output/tracts_$year.fgb" ]; then
    sh scripts/assets.sh raw "$year"
    printf '%s already prepared\n' "$year"
    continue
  fi
  if [ ! -f "scripts/sources-$year.lock.json" ]; then
    sh scripts/lock-year.sh --vintage "$year"
  fi
  sh scripts/download.sh --vintage "$year"
  sh scripts/prepare.sh --vintage "$year"
done
