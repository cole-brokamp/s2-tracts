# s2-tracts

Look up a US Census tract GEOID for a level-30 S2 cell hexadecimal token using a selected annual TIGER/Line boundary vintage (default: 2020).
Each S2 cell is linked to the census tract whose polygon strictly contains the cell's center point; points on a tract boundary, outside all tracts, or inside multiple tracts return no match.

## Install

On macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/cole-brokamp/s2-tracts/main/install.sh | sh -s -- v0.4.0
```

The version argument pins the binary and its matching tract-data release; replace `v0.4.0` with another published version to install it.
The installer verifies the binary, downloads and verifies *2020* tract data, and places the CLI at `~/.local/bin/s2-tracts`.
(Set `S2_TRACTS_BIN_DIR` to use a different binary directory.)

## Look up tracts

Pass level-30 S2 cell hexadecimal tokens as arguments to return one JSON object per cell:

```sh
s2-tracts 89e6537b3a410e3f 89e6537b3a410e41
```

```json
{"s2_cell":"89e6537b3a410e3f","census_tract_id":"09003502100","census_tract_id_vintage":2020}
{"s2_cell":"89e6537b3a410e41","census_tract_id":"09003502100","census_tract_id_vintage":2020}
```

## Census tract boundary vintages

Annual TIGER/Line files describe the boundaries for their stated vintage.
Tract codes and boundaries can change between years, especially around a decennial census.
In general, identifiers from 2010-2019 are generally not compatible with boundaries from 2020 onward.
Choose the vintage matching the data you intend to join; 2020 is only the default, not a universal match for later or earlier data.

```sh
s2-tracts --vintage 2019 89e6537b3a410e3f
```

```json
{"s2_cell":"89e6537b3a410e3f","census_tract_id":"09003502100","census_tract_id_vintage":2019}
```

The first lookup for another vintage downloads a Zstandard-compressed national file and expands it to an indexed FlatGeobuf; later lookups use the cached file offline.

## Manage downloaded data

To download a vintage before lookup:

```sh
s2-tracts data install --vintage 2020
s2-tracts data install --vintage 2019
```

`s2-tracts data path --vintage 2019` prints that year's file path.
Each vintage lives in a separate directory under `~/.local/share/s2-tracts` or `$XDG_DATA_HOME/s2-tracts`.
The selected vintage must have a prepared asset in the release matching your CLI version.
Use `s2-tracts --help` for the supported year range.

## Using

### Use a file as input

Save one token per line in `cells.txt`:

```text
89e6537b3a410e3f
89e6537b3a410e41
```

Read that file and save the JSON Lines output:

```sh
s2-tracts --vintage 2020 < cells.txt > tracts.jsonl
```

`<` reads standard input from the file; `>` writes the results to a file. Results stay in input order, including duplicate cells.

### Call from R

From R, pass an `s2_cell` vector as character tokens and read the JSON Lines output with `jsonlite`:

```r
cells <- s2::as_s2_cell(s2::s2_lnglat(
  c(-84.5, -84.4), c(39.1, 39.2)
))

lines <- system2(
  path.expand("~/.local/bin/s2-tracts"),
  c("--vintage", "2020"),
  input = as.character(cells), stdout = TRUE
)
tracts <- jsonlite::stream_in(textConnection(lines), verbose = FALSE)
```
