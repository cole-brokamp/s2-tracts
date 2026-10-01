library(jsonlite)

status <- suppressWarnings(system2(
  "gh",
  "--version",
  stdout = FALSE,
  stderr = FALSE
))
if (status != 0L) {
  stop(
    "GitHub CLI (gh) is unavailable. Install it before using this script.",
    call. = FALSE
  )
}

release_data <- function(year, tag) {
  stopifnot(length(year) == 1L, is.numeric(year), year %in% 2010:2025)
  stopifnot(
    length(tag) == 1L,
    grepl("^v[0-9]+[.][0-9]+[.][0-9]+$", tag)
  )
  manifest <- file.path("manifests", "assets.lock.json")
  record <- fromJSON(
    manifest,
    simplifyVector = FALSE
  )[[as.character(year)]]
  directory <- file.path(
    "dist",
    paste0("census-tracts-", year, "-r2")
  )
  file <- file.path(directory, paste0("tracts_", year, ".fgb"))
  archive <- paste0(file, ".zst")
  writeLines(
    paste(record$raw_sha256, basename(file), sep = "  "),
    paste0(file, ".sha256")
  )
  writeLines(
    paste(record$archive_sha256, basename(archive), sep = "  "),
    paste0(archive, ".sha256")
  )

  temporary <- tempfile("tract-release-")
  dir.create(temporary)
  on.exit(unlink(temporary, recursive = TRUE), add = TRUE)
  provenance <- file.path(temporary, paste0("provenance_", year, ".json"))
  stopifnot(file.copy(file.path(directory, "provenance.json"), provenance))
  args <- c(
    "release",
    "upload",
    tag,
    archive,
    paste0(file, ".sha256"),
    paste0(archive, ".sha256"),
    provenance,
    "--repo",
    "cole-brokamp/s2-tracts",
    "--clobber"
  )
  message("Uploading ", year, " data to ", tag)
  status <- system2("gh", shQuote(args))
  if (status != 0L) {
    stop("gh release upload failed")
  }
  invisible(args)
}

tag <- Sys.getenv("S2_TRACTS_RELEASE_TAG")
if (!grepl("^v[0-9]+[.][0-9]+[.][0-9]+$", tag)) {
  stop(
    "Set S2_TRACTS_RELEASE_TAG to a version tag, such as v0.3.0, before running this script.",
    call. = FALSE
  )
}

for (year in 2010:2025) {
  release_data(year, tag)
}
