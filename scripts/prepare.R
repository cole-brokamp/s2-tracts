#!/usr/bin/env Rscript
# Prepare indexed polygon components without transforming or repairing geometry.
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(), value = TRUE)[[1]])), "common.R"))
args <- options_from_args(list(
  sources = file.path(project_root, "work/sources"),
  output = file.path(project_root, "dist", paste0(bundle_title, "-r"))
))

unwrap_ring <- function(ring) {
  require_ok(is.matrix(ring) && ncol(ring) == 2L && nrow(ring) >= 4L &&
               all(is.finite(ring)) && all(abs(ring[, 1]) <= 180) &&
               all(abs(ring[, 2]) <= 90) && all(ring[1, ] == ring[nrow(ring), ]),
             "Invalid, non-2D, or unclosed source ring")
  shift <- 0
  for (i in 2:nrow(ring)) {
    x <- ring[i, 1]
    while (x + shift - ring[i - 1L, 1] > 180) shift <- shift - 360
    while (x + shift - ring[i - 1L, 1] < -180) shift <- shift + 360
    if (shift != 0) ring[i, 1] <- x + shift
  }
  require_ok(all(ring[1, ] == ring[nrow(ring), ]), "Ring winds around the globe")
  ring
}

midpoint <- function(ring) mean(range(ring[, 1]))
shift_ring <- function(ring, offset) {
  if (offset != 0) ring[, 1] <- ring[, 1] + offset
  ring
}
normalize_polygon <- function(polygon) {
  require_ok(length(polygon) > 0L, "Empty polygon")
  exterior <- unwrap_ring(polygon[[1]])
  exterior <- shift_ring(exterior, -360 * floor((midpoint(exterior) + 180) / 360))
  require_ok(diff(range(exterior[, 1])) < 180, "Polygon too wide for Rust lookup")
  rings <- list(exterior)
  for (hole in polygon[-1]) {
    hole <- unwrap_ring(hole)
    rings[[length(rings) + 1L]] <- shift_ring(
      hole, 360 * round((midpoint(exterior) - midpoint(hole)) / 360))
  }
  rings
}
normalize_geometry <- function(geometry) {
  if (inherits(geometry, "POLYGON")) return(sf::st_polygon(normalize_polygon(geometry)))
  require_ok(inherits(geometry, "MULTIPOLYGON") && length(geometry) > 0L,
             "Missing or non-polygon source geometry")
  sf::st_multipolygon(lapply(geometry, normalize_polygon))
}

prepare_year <- function(year, records, staging) {
  records <- records[records$year == year, ]
  parts <- vector("list", nrow(records))
  source_ids <- character()
  source_info <- vector("list", nrow(records))
  for (i in seq_len(nrow(records))) {
    record <- records[i, ]
    path <- normalizePath(file.path(args$sources, record$filename))
    layer <- sub("\\.zip$", ".shp", record$filename)
    data <- sf::st_read(paste0("/vsizip/", path, "/", layer), quiet = TRUE)
    require_ok(isTRUE(sf::st_is_longlat(data)), paste("Expected geographic CRS:", path))
    source_crs <- sf::st_crs(data)$wkt
    field <- if (year == 2010) "GEOID10" else "GEOID"
    ids <- data[[field]]
    require_ok(is.character(ids) && length(ids) == nrow(data) && length(ids) > 0L &&
                 all(!is.na(ids) & grepl("^[0-9]{11}$", ids) & substr(ids, 1, 2) == record$state) &&
                 !anyDuplicated(c(source_ids, ids)), paste("Invalid or duplicate GEOIDs:", path))
    source_ids <- c(source_ids, ids)
    # Omit CRS on working geometry so sf uses planar GEOS and never spherical s2.
    geometry <- sf::st_sfc(lapply(sf::st_geometry(data), normalize_geometry))
    require_ok(all(sf::st_is_valid(geometry)) && !any(sf::st_is_empty(geometry)),
               paste("Invalid normalized geometry; no repair permitted:", path))
    components <- suppressWarnings(sf::st_cast(sf::st_sf(GEOID = ids, geometry = geometry), "POLYGON"))
    require_ok(all(as.numeric(sf::st_area(components)) > 0), paste("Zero-area component:", path))
    parts[[i]] <- components
    source_info[[i]] <- c(as.list(record), list(crs = source_crs, tracts = length(ids),
                                               components = nrow(components)))
    message(year, " ", record$state, ": ", length(ids), " tracts, ", nrow(components), " components")
  }
  message(year, ": combining components and writing the national spatial index")
  data <- do.call(rbind, parts)
  require_ok(setequal(substr(data$GEOID, 1, 2), records$state) &&
               setequal(data$GEOID, source_ids), "Incomplete geographic coverage")
  name <- paste0("tracts_", year)
  path <- file.path(staging, paste0(name, ".fgb"))
  sf::st_write(data, path, layer = name, driver = "FlatGeobuf", quiet = TRUE,
               layer_options = c("SPATIAL_INDEX=YES", paste0("TITLE=", bundle_title)))
  list(year = year, file = basename(path), tracts = length(source_ids),
       components = nrow(data), bytes = file.info(path)$size, sha256 = sha256(path),
       sources = source_info)
}

require_ok(!file.exists(args$output), paste("Refusing to overwrite:", args$output))
staging <- paste0(args$output, ".building")
require_ok(!file.exists(staging), paste("Inspect existing staging directory before retrying:", staging))
records <- source_lock()
for (i in seq_len(nrow(records))) {
  verify_source(file.path(args$sources, records$filename[[i]]), records[i, ])
}
require_ok(dir.create(staging, recursive = TRUE), paste("Cannot create:", staging))
# Failures leave the .building directory for inspection; only complete results move into place.
years <- lapply(c(2010, 2020), prepare_year, records = records, staging = staging)
provenance <- list(
  bundle = bundle_title, years = years,
  preparation = list(R = R.version.string, sf = as.character(utils::packageVersion("sf")),
                     libraries = as.list(sf::sf_extSoftVersion())),
  coordinate_policy = paste("Source longitude/latitude without datum transformation;",
                            "integer-360 longitude unwrapping and hole alignment only;",
                            "no repair, simplification, clipping, or rounding.")
)
jsonlite::write_json(provenance, file.path(staging, "provenance.json"),
                     auto_unbox = TRUE, pretty = TRUE, digits = NA)
require_ok(file.rename(staging, args$output), paste("Cannot finalize:", args$output))
message("Prepared ", normalizePath(args$output))
