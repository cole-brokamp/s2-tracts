# Maintainer documentation

## Lookup and bundle contract

The CLI accepts unsigned level-30 S2 cell IDs and always queries both the fixed 2010 and 2020 decennial tract vintages.
For each vintage independently, a point must be strictly inside exactly one distinct GEOID.
Outside points, exterior or hole boundaries, and overlaps between different tracts yield `null`.
A boundary wins even if another tract contains the point.
Multiple components of one GEOID count once.
There is no nearest assignment or distance tolerance.

S2 centers are compared directly with source longitude and latitude using planar edges, without a datum transformation.
Antimeridian components are unwrapped so both sides can be queried.
The FlatGeobuf spatial indexes remain on disk; the reader loads candidate features instead of a national index.
Opening validates the FlatGeobuf schema and bundle title.
Lookup does not hash all national files or scan unvisited feature bytes.

The release data assets are `tracts_2010.fgb`, `tracts_2020.fgb`, and `provenance.json`.
The first two are required by the CLI.
The bundle covers all 50 states, DC, Puerto Rico, American Samoa, Guam, the Northern Mariana Islands, and the US Virgin Islands, including water tracts.
2010 tracts come from the [TIGER2010 state tract archives](https://www2.census.gov/geo/tiger/TIGER2010/TRACT/2010/) (`GEOID10`).
2020 tracts come from the [TIGER2020 state tract archives](https://www2.census.gov/geo/tiger/TIGER2020/TRACT/) (`GEOID`).
`scripts/sources.lock.json` lists the 112 exact source URLs, sizes, and SHA-256 hashes.
`provenance.json` records prepared file hashes, counts, and source details.

## Rebuild the tract bundle

Preparation needs R packages `sf`, `jsonlite`, and `digest`.
The `sf` installation needs GDAL with GEOS and the FlatGeobuf driver.
These are maintainer dependencies, not CLI runtime dependencies.

```sh
Rscript scripts/download.R
Rscript scripts/prepare.R
```

The downloader verifies cached source archives in `work/sources`.
Preparation writes to `dist/census-tracts-2010-2020-r1-r` and refuses to overwrite an existing output directory.
The FlatGeobuf files contain 2D polygon components with one string `GEOID` column and built-in spatial indexes.
Preparation preserves source coordinates except integer-360 longitude unwrapping and hole alignment.
It performs no geometry repair, simplification, clipping, or rounding.

Do not change the data files without updating the fixed SHA-256 and byte counts in `src/main.rs` and releasing a new version.
A developer can point `data install` at a local or mirrored versioned asset URL by setting `S2_TRACTS_RELEASE_BASE_URL`.

## Release

This checkout is intended for `cole-brokamp/s2-tracts`; the installer becomes usable only after the source and release assets are published.

1. Create and push a tag matching `Cargo.toml`, such as `v0.1.0`.
2. Run the **Build release binaries** GitHub Actions workflow with that tag to create a draft release and upload four platform binaries and SHA-256 files.
3. Upload `tracts_2010.fgb`, `tracts_2020.fgb`, and `provenance.json` from `dist/census-tracts-2010-2020-r1-r` to the same draft release:

   ```sh
   gh release upload v0.1.0 dist/census-tracts-2010-2020-r1-r/* --repo cole-brokamp/s2-tracts
   ```

4. Verify the binary, checksum, and data assets, publish the draft release, and confirm the README install command on a clean machine.

The installer fetches the latest published binary, verifies its SHA-256, and runs `data install` before placing the binary at `~/.local/bin/s2-tracts`.
`data install` downloads the two FlatGeobuf files from the versioned GitHub release matching the binary version, verifies their fixed sizes and SHA-256 hashes, and atomically places them in a persistent user data directory.
