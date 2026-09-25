#!/usr/bin/env Rscript
# Freeze the 56 Census source archives for a new annual tract vintage.
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(), value = TRUE)[[1]])), "common.R"))
args <- options_from_args(list(vintage = "", sources = file.path(project_root, "work/sources")))
year <- check_vintage(args$vintage)
lock <- file.path(project_root, "scripts", paste0("sources-", year, ".lock.json"))
require_ok(!file.exists(lock), paste("Refusing to overwrite:", lock))
require_ok(dir.create(args$sources, recursive = TRUE, showWarnings = FALSE) || dir.exists(args$sources),
           paste("Cannot create:", args$sources))
base <- if (year == 2010L) {
  "https://www2.census.gov/geo/tiger/TIGER2010/TRACT/2010/"
} else {
  paste0("https://www2.census.gov/geo/tiger/TIGER", year, "/TRACT/")
}
options(timeout = max(120, getOption("timeout")))
records <- lapply(states, function(state) {
  filename <- sprintf("tl_%s_%s_tract%s.zip", year, state,
                      if (year == 2010L) "10" else "")
  url <- paste0(base, filename)
  path <- file.path(args$sources, filename)
  if (!valid_source_archive(path, filename)) {
    if (file.exists(path)) unlink(path)
    partial <- paste0(path, ".part")
    for (attempt in seq_len(8L)) {
      actual_url <- if (attempt == 1L) url else sub("/([^/]+)$", "//\\1", url)
      error <- tryCatch({
        status <- download.file(actual_url, partial, mode = "wb", quiet = TRUE)
        require_ok(status == 0L && valid_source_archive(partial, filename),
                   paste("Download is not the expected shapefile archive:", actual_url))
        require_ok(file.rename(partial, path), paste("Cannot finalize:", path))
        NULL
      }, error = identity)
      if (is.null(error)) break
      unlink(partial)
      if (attempt == 8L) stop(error)
      message(conditionMessage(error), "; retrying")
      Sys.sleep(min(60, 5 * attempt))
    }
  }
  message(year, " ", state, ": locked")
  list(year = year, state = state, url = url, filename = filename,
       bytes = unname(file.info(path)$size), sha256 = sha256(path))
})
partial <- paste0(lock, ".building")
require_ok(!file.exists(partial), paste("Inspect existing staging file:", partial))
jsonlite::write_json(records, partial, auto_unbox = TRUE, pretty = TRUE, digits = NA)
require_ok(file.rename(partial, lock), paste("Cannot finalize:", lock))
message("Locked ", lock)
