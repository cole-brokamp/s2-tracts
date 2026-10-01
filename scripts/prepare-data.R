library(stow)
library(jsonlite)
library(digest)

replace_manifest <- tolower(Sys.getenv("S2_TRACTS_REPLACE_MANIFEST")) == "true"
if (!replace_manifest) {
  message(
    "Asset manifest replacement is disabled. To allow an intentional repin,",
    "set the environment variable S2_TRACTS_REPLACE_MANIFEST=true"
  )
}

# fmt: skip
states <- c(
  "01", "02", "04", "05", "06", "08", "09", "10", "11", "12", "13", "15",
  "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27",
  "28", "29", "30", "31", "32", "33", "34", "35", "36", "37", "38", "39",
  "40", "41", "42", "44", "45", "46", "47", "48", "49", "50", "51", "53",
  "54", "55", "56", "60", "66", "69", "72", "78"
)

years <- 2010:2025

prepare_year <- function(year, offline = FALSE, output = NULL) {
  stopifnot(length(year) == 1L, is.numeric(year), year %in% years)
  if (is.null(output)) {
    output <- file.path(
      "dist",
      paste0("census-tracts-", year, "-r2")
    )
  }
  filenames <- sprintf(
    "tl_%s_%s_tract%s.zip",
    year,
    states,
    if (year == 2010) "10" else ""
  )
  urls <- paste0(
    "https://www2.census.gov/geo/tiger/TIGER",
    year,
    "/TRACT/",
    if (year == 2010) "2010/" else "",
    filenames
  )
  paths <- vapply(
    urls,
    function(url) {
      stow(
        url,
        package = "io.geomarker.s2tract",
        subdir = as.character(year),
        etag = FALSE,
        offline = offline,
        quiet = TRUE
      )
    },
    character(1)
  )

  staging <- tempfile("tract-sources-")
  dir.create(staging)
  on.exit(unlink(staging, recursive = TRUE), add = TRUE)
  stopifnot(all(file.symlink(paths, file.path(staging, filenames))))

  status <- system2(
    "cargo",
    shQuote(c(
      "build",
      "--offline",
      "--locked",
      "--release",
      "--features",
      "data-prep",
      "--bin",
      "prepare-tracts"
    ))
  )
  if (status != 0L) {
    stop("cargo build failed")
  }
  status <- system2(
    file.path("target", "release", "prepare-tracts"),
    shQuote(c("--vintage", year, "--sources", staging, "--output", output))
  )
  if (status != 0L) {
    stop("prepare-tracts failed")
  }
  invisible(output)
}

# Running or sourcing this script prepares and packages every supported year.
for (year in years) {
  file <- file.path(
    "dist",
    paste0("census-tracts-", year, "-r2"),
    paste0("tracts_", year, ".fgb")
  )
  if (file.exists(file)) {
    message(year, ": already prepared")
  } else {
    prepare_year(year)
  }
}
# Compress the prepared files and record their sizes and hashes.
manifest <- file.path("manifests", "assets.lock.json")
staging <- paste0(manifest, ".building")
stopifnot(!file.exists(staging))
records <- setNames(
  lapply(years, function(year) {
    directory <- file.path("dist", paste0("census-tracts-", year, "-r2"))
    file <- file.path(directory, paste0("tracts_", year, ".fgb"))
    archive <- paste0(file, ".zst")
    if (!file.exists(archive)) {
      temporary <- paste0(archive, ".building")
      stopifnot(!file.exists(temporary))
      status <- system2(
        "zstd",
        shQuote(c("-9", "--quiet", file, "-o", temporary))
      )
      if (status != 0L) {
        stop("zstd compression failed")
      }
      stopifnot(file.rename(temporary, archive))
    }
    message(year, ": recording asset sizes and hashes")
    list(
      raw_bytes = unname(file.info(file)$size),
      raw_sha256 = digest(file = file, algo = "sha256"),
      archive_bytes = unname(file.info(archive)$size),
      archive_sha256 = digest(file = archive, algo = "sha256")
    )
  }),
  as.character(years)
)
expected <- if (file.exists(manifest)) {
  fromJSON(manifest, simplifyVector = FALSE)
} else {
  NULL
}
if (!isTRUE(all.equal(records, expected, tolerance = 0))) {
  write_json(
    records,
    staging,
    auto_unbox = TRUE,
    pretty = TRUE,
    digits = NA
  )
  if (file.exists(manifest) && !replace_manifest) {
    stop(
      "Asset manifest differs. Inspect ",
      staging,
      "; to accept the new values, remove that staging file and rerun setting",
      "environment variable S2_TRACTS_REPLACE_MANIFEST=true",
      call. = FALSE
    )
  }
  stopifnot(file.rename(staging, manifest))
}
for (year in years) {
  directory <- file.path(
    "dist",
    paste0("census-tracts-", year, "-r2")
  )
  file <- file.path(directory, paste0("tracts_", year, ".fgb"))
  archive <- paste0(file, ".zst")
  record <- records[[as.character(year)]]
  writeLines(
    paste(record$raw_sha256, basename(file), sep = "  "),
    paste0(file, ".sha256")
  )
  writeLines(
    paste(record$archive_sha256, basename(archive), sep = "  "),
    paste0(archive, ".sha256")
  )
}
