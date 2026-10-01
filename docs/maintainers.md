# Releasing

Run from the project root. Preparation needs R with `stow` (>= 0.3.0), `jsonlite`, and `digest`; Rust; `zstd`; SHA-256 tools; and GDAL/GEOS command-line tools (`ogrinfo`, `ogr2ogr`, `gdal-config`, `geos-config`).
Uploads need an authenticated GitHub CLI (`gh`).

Prepare and package all years, then test:

```sh
Rscript scripts/prepare-data.R
cargo test --offline --locked --features data-prep
```

For an intentional asset repin, use `S2_TRACTS_REPLACE_MANIFEST=true Rscript scripts/prepare-data.R`.

Commit changes, create and push a tag matching `Cargo.toml`, then run **Build release binaries** in GitHub Actions with that tag to create the draft release. Upload all years to it:

```sh
S2_TRACTS_RELEASE_TAG=v0.4.0 Rscript scripts/release-data.R
```

Uploads replace same-named assets.
Check the assets, publish the draft, and verify the README installer on a clean machine.
