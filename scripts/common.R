# Shared paths, source lock, and command-line options for the maintainer scripts.
script_arg <- grep("^--file=", commandArgs(), value = TRUE)
project_root <- dirname(dirname(normalizePath(sub("^--file=", "", script_arg[[1]]))))
bundle_title <- "census-tracts-2010-2020-r1"

require_ok <- function(ok, message) {
  if (!isTRUE(ok)) stop(message, call. = FALSE)
}

options_from_args <- function(defaults) {
  args <- commandArgs(trailingOnly = TRUE)
  require_ok(length(args) %% 2L == 0L, "Use --option value pairs")
  for (i in seq_len(length(args) / 2L) * 2L - 1L) {
    key <- sub("^--", "", args[[i]])
    require_ok(startsWith(args[[i]], "--") && key %in% names(defaults),
               paste("Unknown option:", args[[i]]))
    defaults[[key]] <- args[[i + 1L]]
  }
  defaults
}

sha256 <- function(path) digest::digest(file = path, algo = "sha256")

source_lock <- function() {
  records <- jsonlite::fromJSON(file.path(project_root, "scripts/sources.lock.json"))
  states <- strsplit(paste("01 02 04 05 06 08 09 10 11 12 13 15 16 17 18 19 20",
                           "21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37",
                           "38 39 40 41 42 44 45 46 47 48 49 50 51 53 54 55 56",
                           "60 66 69 72 78"), " ")[[1]]
  expected <- as.vector(outer(c(2010, 2020), states, paste, sep = ":"))
  require_ok(nrow(records) == 112L &&
               setequal(paste(records$year, records$state, sep = ":"), expected),
             "Source lock must contain the 112 decennial state archives")
  filenames <- sprintf("tl_%s_%s_tract%s.zip", records$year, records$state,
                       ifelse(records$year == 2010, "10", ""))
  bases <- ifelse(records$year == 2010,
                 "https://www2.census.gov/geo/tiger/TIGER2010/TRACT/2010/",
                 "https://www2.census.gov/geo/tiger/TIGER2020/TRACT/")
  require_ok(all(records$filename == filenames & records$url == paste0(bases, filenames)),
             "Source lock has an incompatible URL or filename")
  records[order(records$year, records$state), ]
}

verify_source <- function(path, record) {
  require_ok(file.exists(path) && file.info(path)$size == record$bytes &&
               sha256(path) == record$sha256,
             paste("Source missing or differs from frozen checksum:", path))
}
