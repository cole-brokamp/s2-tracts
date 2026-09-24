# s2-tracts

Look up the 2010 and 2020 US Census tract GEOIDs for unsigned level-30 S2 cell IDs from your terminal.

## Install

On macOS or Linux, paste this command into a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/cole-brokamp/s2-tracts/main/install.sh | sh
```

The installer downloads the CLI and tract data (about 1.3 GB).
It places the command at `~/.local/bin/s2-tracts` and prints a PATH instruction if needed.
Once installed, lookups work offline.
No R installation is needed.

## Look up tracts

```sh
s2-tracts 9936721416563002943
```

The command returns one JSON object per ID:

```json
{"s2_cell_id":"9936721416563002943","tract_2010":"09003502100","tract_2020":"09003502100"}
```

Pass multiple IDs as arguments, or pipe one ID per line on standard input:

```sh
printf '%s\n' 9936721416563002943 | s2-tracts > tracts.jsonl
```

Results stay in input order, including duplicate IDs.
IDs are strings so large S2 values and leading zeros in GEOIDs stay intact.
A `null` tract means that vintage has no unambiguous strict match at the cell center.
Points on tract boundaries have no match; the tool does not choose a nearest tract.
Invalid or non-level-30 S2 IDs produce an error and no output.

## Manage downloaded data

The installer downloads both tract vintages.
To verify and reuse the downloaded data later, run:

```sh
s2-tracts data install
```

By default, the data is installed in `~/.local/share/s2-tracts/s2-tracts-2010-2020-r1`.
If `XDG_DATA_HOME` is set, the path is `$XDG_DATA_HOME/s2-tracts/s2-tracts-2010-2020-r1` instead.
`s2-tracts data path` prints the effective location on your computer.

Set `XDG_DATA_HOME` before installation if you want the data under another parent directory.
