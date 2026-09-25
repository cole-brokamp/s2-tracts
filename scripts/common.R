# Shared paths, source lock, and command-line options for the maintainer scripts.
script_arg <- grep("^--file=", commandArgs(), value = TRUE)
project_root <- dirname(dirname(normalizePath(sub("^--file=", "", script_arg[[1]]))))
bundle_title <- function(vintage) paste0("census-tracts-", vintage, "-r1")
states <- strsplit(paste("01 02 04 05 06 08 09 10 11 12 13 15 16 17 18 19 20",
                         "21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37",
                         "38 39 40 41 42 44 45 46 47 48 49 50 51 53 54 55 56",
                         "60 66 69 72 78"), " ")[[1]]

check_vintage <- function(value) {
  year <- suppressWarnings(as.integer(value))
  require_ok(!is.na(year) && year >= 2010L && year <= 2025L &&
               identical(as.character(year), as.character(value)),
             "Vintage must be an integer from 2010 through 2025")
  year
}

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

valid_source_archive <- function(path, filename) {
  if (!file.exists(path) || file.info(path)$size < 1000) return(FALSE)
  members <- tryCatch(suppressWarnings(utils::unzip(path, list = TRUE)), error = function(e) NULL)
  !is.null(members) && sub("\\.zip$", ".shp", filename) %in% members$Name
}

source_lock <- function(vintage) {
  path <- file.path(project_root, "scripts", paste0("sources-", vintage, ".lock.json"))
  if (file.exists(path)) {
    records <- jsonlite::fromJSON(path)
  } else {
    require_ok(vintage %in% c(2010L, 2020L),
               paste("Source lock missing for", vintage, "; run scripts/lock-year.R first"))
    records <- jsonlite::fromJSON(file.path(project_root, "scripts/sources.lock.json"))
    records <- records[records$year == vintage, ]
  }
  expected <- paste(vintage, states, sep = ":")
  require_ok(nrow(records) == length(states) &&
               setequal(paste(records$year, records$state, sep = ":"), expected),
             "Source lock must contain all 56 state and territory archives")
  filenames <- sprintf("tl_%s_%s_tract%s.zip", vintage, records$state,
                       if (vintage == 2010L) "10" else "")
  base <- if (vintage == 2010L) {
    "https://www2.census.gov/geo/tiger/TIGER2010/TRACT/2010/"
  } else {
    paste0("https://www2.census.gov/geo/tiger/TIGER", vintage, "/TRACT/")
  }
  require_ok(all(records$filename == filenames & records$url == paste0(base, filenames)),
             "Source lock has an incompatible URL or filename")
  records[order(records$state), ]
}

verify_source <- function(path, record) {
  require_ok(file.exists(path) && file.info(path)$size == record$bytes &&
               sha256(path) == record$sha256 &&
               valid_source_archive(path, record$filename),
             paste("Source missing or differs from frozen checksum:", path))
}
