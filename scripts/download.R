#!/usr/bin/env Rscript
# Download and verify one annual TIGER/Line tract vintage.
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(), value = TRUE)[[1]])), "common.R"))
args <- options_from_args(list(vintage = "2020", sources = file.path(project_root, "work/sources")))
vintage <- check_vintage(args$vintage)
dir.create(args$sources, recursive = TRUE, showWarnings = FALSE)
records <- source_lock(vintage)
options(timeout = max(120, getOption("timeout")))

for (i in seq_len(nrow(records))) {
  record <- records[i, ]
  path <- file.path(args$sources, record$filename)
  if (!file.exists(path)) {
    partial <- paste0(path, ".part")
    for (attempt in seq_len(8L)) {
      url <- if (attempt == 1L) record$url else sub("/([^/]+)$", "//\\1", record$url)
      error <- tryCatch({
        status <- download.file(url, partial, mode = "wb", quiet = TRUE)
        require_ok(status == 0L, paste("Download failed:", url))
        verify_source(partial, record)
        require_ok(file.rename(partial, path), paste("Cannot finalize:", path))
        NULL
      }, error = identity)
      if (is.null(error)) break
      unlink(partial)
      if (attempt == 8L) stop(error)
      message(conditionMessage(error), "; retrying")
      Sys.sleep(min(120, 15 * attempt))
    }
  }
  verify_source(path, record)
  message(record$year, " ", record$state, ": verified")
}
