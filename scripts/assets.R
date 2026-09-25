#!/usr/bin/env Rscript
# Compress prepared vintages, pin hashes, and verify assets before release.
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(), value = TRUE)[[1]])), "common.R"))

args <- commandArgs(trailingOnly = TRUE)
require_ok(length(args) >= 1L && args[[1]] %in% c("freeze", "verify"),
           "Usage: Rscript scripts/assets.R freeze [--replace] | verify YEAR FILE ARCHIVE")

prepared_file <- function(year) {
  folder <- file.path(project_root, "dist", bundle_title(year))
  file.path(folder, paste0("tracts_", year, ".fgb"))
}

verified_raw <- function(year, file) {
  require_ok(file.exists(file), paste("Missing prepared file:", file))
  provenance_path <- file.path(dirname(file), "provenance.json")
  require_ok(file.exists(provenance_path), paste("Missing provenance:", provenance_path))
  provenance <- jsonlite::read_json(provenance_path, simplifyVector = FALSE)
  records <- Filter(function(item) identical(as.integer(item$year), year), provenance$years)
  require_ok(length(records) == 1L, paste("Expected one provenance record for", year))
  size <- unname(file.info(file)$size)
  hash <- sha256(file)
  require_ok(identical(as.numeric(records[[1]]$bytes), size) &&
               identical(records[[1]]$sha256, hash),
             paste("Prepared file differs from provenance:", file))
  list(raw_bytes = size, raw_sha256 = hash)
}

verified_archive <- function(file) {
  require_ok(file.exists(file), paste("Missing compressed file:", file))
  status <- system2("zstd", c("--test", "--quiet", shQuote(file)))
  require_ok(identical(status, 0L), paste("Invalid compressed file:", file))
  list(archive_bytes = unname(file.info(file)$size), archive_sha256 = sha256(file))
}

same_record <- function(actual, expected) {
  keys <- c("raw_bytes", "raw_sha256", "archive_bytes", "archive_sha256")
  is.list(expected) && identical(names(actual), names(expected)) &&
    all(vapply(keys, function(key) as.character(actual[[key]]) == as.character(expected[[key]]),
               logical(1)))
}

manifest_path <- file.path(project_root, "scripts/assets.lock.json")
if (identical(args[[1]], "freeze")) {
  require_ok(length(args) == 1L ||
               (length(args) == 2L && identical(args[[2]], "--replace")),
             "Usage: Rscript scripts/assets.R freeze [--replace]")
  replace <- length(args) == 2L
  assets <- list()
  for (year in 2010:2025) {
    file <- prepared_file(year)
    raw <- verified_raw(year, file)
    archive <- paste0(file, ".zst")
    if (!file.exists(archive)) {
      status <- system2("zstd", c("-9", "--quiet", shQuote(file), "-o", shQuote(archive)))
      require_ok(identical(status, 0L), paste("Could not compress:", file))
    }
    compressed <- verified_archive(archive)
    assets[[as.character(year)]] <- c(raw, compressed)
    message(year, " ", raw$raw_bytes, " ", compressed$archive_bytes)
  }
  if (file.exists(manifest_path)) {
    pinned <- jsonlite::read_json(manifest_path, simplifyVector = FALSE)
    same <- identical(names(assets), names(pinned)) &&
      all(vapply(names(assets), function(year) same_record(assets[[year]], pinned[[year]]),
                 logical(1)))
    require_ok(same || replace,
               paste("Asset manifest differs; inspect before replacing:", manifest_path))
  } else {
    same <- FALSE
  }
  if (!same) {
    staging <- paste0(manifest_path, ".building")
    require_ok(!file.exists(staging), paste("Inspect existing staging file:", staging))
    jsonlite::write_json(assets, staging, auto_unbox = TRUE, pretty = TRUE, digits = NA)
    require_ok(file.rename(staging, manifest_path), paste("Cannot finalize:", manifest_path))
  }
  message("Pinned ", manifest_path)
} else {
  require_ok(length(args) == 4L, "Usage: Rscript scripts/assets.R verify YEAR FILE ARCHIVE")
  year <- check_vintage(args[[2]])
  file <- args[[3]]
  archive <- args[[4]]
  require_ok(basename(file) == paste0("tracts_", year, ".fgb") &&
               archive == paste0(file, ".zst"), "Unexpected asset names")
  raw <- verified_raw(year, file)
  compressed <- verified_archive(archive)
  require_ok(file.exists(manifest_path), paste("Missing asset manifest:", manifest_path))
  pinned <- jsonlite::read_json(manifest_path, simplifyVector = FALSE)[[as.character(year)]]
  require_ok(same_record(c(raw, compressed), pinned),
             paste("Release asset differs from pinned manifest:", file))
  writeLines(paste(raw$raw_sha256, basename(file), sep = "  "), paste0(file, ".sha256"))
  writeLines(paste(compressed$archive_sha256, basename(archive), sep = "  "),
             paste0(archive, ".sha256"))
}
