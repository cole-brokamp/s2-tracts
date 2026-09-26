# Maintainer documentation

## Lookup and data contract

The CLI accepts unsigned level-30 S2 cell IDs and queries one annual TIGER/Line tract vintage at a time.
The default is 2020; `--vintage YEAR` selects 2010 through 2025.
For each point, a match must be strictly inside exactly one distinct GEOID.
Outside points, exterior or hole boundaries, and overlaps between different tracts yield `null`.
A boundary wins even if another tract contains the point.
Multiple components of one GEOID count once.
There is no nearest assignment or distance tolerance.

S2 centers are compared directly with source longitude and latitude using planar edges, without a datum transformation.
Antimeridian components are unwrapped so both sides can be queried.
The FlatGeobuf spatial index remains on disk; the reader loads candidate features instead of a national index.
Opening validates the FlatGeobuf schema, selected vintage, and preparation revision.
Lookup does not hash national files or scan unvisited feature bytes.

The release data assets for a year are `tracts_YEAR.fgb.zst`, `tracts_YEAR.fgb.zst.sha256`, `tracts_YEAR.fgb.sha256`, and `provenance_YEAR.json`.
The CLI pins the sizes and SHA-256 hashes of both the compressed download and expanded FlatGeobuf in `scripts/assets.lock.json` at build time.
Every newly prepared file uses the `census-tracts-YEAR-r2` FlatGeobuf header title and a separate `tracts-YEAR-r2` cache directory.
Each vintage covers all 50 states, DC, Puerto Rico, American Samoa, Guam, the Northern Mariana Islands, and the US Virgin Islands, including water tracts.
The 2010 source uses `GEOID10`; other annual sources use `GEOID`.
Source records are frozen separately for each year in `scripts/sources-YEAR.lock.json`.
`provenance.json` records prepared file hashes, counts, and source details.

## Prepare an annual vintage

Preparation needs Bash, `curl`, `jq`, `unzip`, `zstd`, SHA-256 tools, Rust, `geos-config`, and GDAL command line tools (`ogrinfo`, `ogr2ogr`, `gdal-config`) with GEOS, GeoJSONSeq, and FlatGeobuf support.
These are maintainer dependencies, not CLI runtime dependencies.

For each released year, the source lock already exists.
For a newly supported year, first create and review its frozen source lock:

```sh
sh scripts/lock-year.sh --vintage 2019
sh scripts/download.sh --vintage 2019
sh scripts/prepare.sh --vintage 2019
```

Preparation refuses to overwrite an existing output directory.
It writes `dist/census-tracts-YEAR-r2/tracts_YEAR.fgb` and `provenance.json`.
The FlatGeobuf contains 2D polygon components with one string `GEOID` column and a spatial index.
Preparation preserves source coordinates except integer-360 longitude unwrapping and hole alignment.
The GDAL stream uses matching source and destination CRS overrides because GeoJSONSeq otherwise changes NAD83 coordinate values on output.
It performs no geometry repair, simplification, clipping, or rounding.

To prepare all annual years, run `sh scripts/build-all-years.sh`.
Compare provenance, tract counts, and spot checks against the official year's source before upload, especially where source schemas or coverage differ.
Run `sh scripts/assets.sh freeze --replace` after all years are rebuilt and checked against the previous vintages; this compresses and pins the exact release assets before building the binaries.
For subsequent runs, use `sh scripts/assets.sh freeze` to verify that the pinned hashes still match.

## Release

1. Create and push a version tag matching `Cargo.toml`, such as `v0.3.0`.
2. Run the **Build release binaries** workflow with that tag; it creates a draft release with four platform binaries and their SHA-256 files. Linux binaries use the musl target so they run on clusters with older glibc versions. The workflow checks that they have no dynamic loader and can start before upload.
3. Upload every prepared year to that draft release with `sh scripts/release-data.sh YEAR v0.3.0`.
4. Verify the binaries, sidecars, and data assets, then publish the draft release and check the README install command on a clean machine.

The installer fetches and verifies the latest published binary without downloading data.
It offers to preinstall the default vintage when a terminal is attached.
On first lookup, the CLI downloads only the selected vintage from the release matching its binary version, verifies both hashes against its embedded manifest and the FlatGeobuf schema, and atomically places the expanded file in the user data directory.
The environment variable `S2_TRACTS_RELEASE_BASE_URL` can point data installation to a local or mirrored versioned asset URL for development.
