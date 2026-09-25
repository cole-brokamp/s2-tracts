#!/bin/sh
set -eu

for year in $(seq 2010 2025); do
  output="dist/census-tracts-$year-r1"
  if [ -f "$output/tracts_$year.fgb" ]; then
    printf '%s already prepared\n' "$year"
    continue
  fi
  if [ ! -f "scripts/sources-$year.lock.json" ]; then
    Rscript scripts/lock-year.R --vintage "$year"
  fi
  Rscript scripts/download.R --vintage "$year"
  Rscript scripts/prepare.R --vintage "$year"
done
