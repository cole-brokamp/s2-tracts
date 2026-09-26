# s2-tracts

Look up a US Census tract GEOID for an unsigned level-30 S2 cell ID using a selected annual TIGER/Line boundary vintage.

## Install

On macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/cole-brokamp/s2-tracts/main/install.sh | sh
```

The installer places the CLI at `~/.local/bin/s2-tracts` and does not require tract data to be downloaded.
When run in a terminal, it offers to preinstall the default 2020 vintage.
Choose no to download data only when you first look up a tract.

## Look up tracts

```sh
s2-tracts 9936721416563002943
s2-tracts --vintage 2019 9936721416563002943
```

The default vintage is **2020**.
The first lookup for a vintage downloads a Zstandard-compressed national file and expands it to an indexed FlatGeobuf; later lookups use the cached file offline.
The download and installed file sizes vary by vintage and release.
The command returns one JSON object per ID:

```json
{"s2_cell_id":"9936721416563002943","tract":"09003502100","vintage":2020}
```

Pass multiple IDs as arguments, or pipe one ID per line on standard input:

```sh
printf '%s\n' 9936721416563002943 9936721416563002945 | s2-tracts --vintage 2020 > tracts.jsonl
```

Results stay in input order, including duplicate IDs.
IDs are strings so large S2 values and leading zeros in GEOIDs stay intact.
A `null` tract means the cell center has no unambiguous strict match in the selected vintage.
Points on tract boundaries have no match; the tool does not choose a nearest tract.
Invalid or non-level-30 S2 IDs produce an error and no output.

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

Annual TIGER/Line files describe the boundaries for their stated vintage.
Tract codes and boundaries can change between years, particularly around a decennial census.
Choose the vintage matching the data you intend to join; 2020 is only the default, not a universal match for later or earlier data.
For example, [Connecticut adopted new county equivalents in the 2022 TIGER/Line vintage](https://www.census.gov/geographies/mapping-files/2022/geo/tiger-line-file.html), changing the county portion of some tract GEOIDs.
