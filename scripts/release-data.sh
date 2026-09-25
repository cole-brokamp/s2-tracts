#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
  echo 'usage: scripts/release-data.sh YEAR TAG' >&2
  exit 2
fi
year=$1
tag=$2
case "$year" in
  201[0-9]|202[0-5]) ;;
  *) echo 'YEAR must be 2010 through 2025' >&2; exit 2 ;;
esac
file="dist/census-tracts-$year-r1/tracts_$year.fgb"
if [ ! -f "$file" ] && { [ "$year" = 2010 ] || [ "$year" = 2020 ]; }; then
  file="dist/census-tracts-2010-2020-r1-r/tracts_$year.fgb"
fi
test -f "$file" || { echo "missing $file" >&2; exit 1; }
archive="$file.zst"
test -f "$archive" || { echo "missing $archive; run python3 scripts/freeze-assets.py" >&2; exit 1; }
provenance="$(dirname "$file")/provenance_$year.json"
python3 - "$year" "$file" "$archive" <<'PY'
import hashlib, json, pathlib, sys
year, name, archive = int(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
provenance = json.loads((name.parent / 'provenance.json').read_text())
record = next(item for item in provenance['years'] if item['year'] == year)
manifest = json.loads(pathlib.Path('scripts/assets.lock.json').read_text())[str(year)]
def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()
raw_hash, archive_hash = digest(name), digest(archive)
if name.stat().st_size != record['bytes'] or raw_hash != record['sha256']:
    raise SystemExit(f'prepared asset differs from provenance: {name}')
if name.stat().st_size != manifest['raw_bytes'] or raw_hash != manifest['raw_sha256']:
    raise SystemExit(f'prepared asset differs from pinned manifest: {name}')
if archive.stat().st_size != manifest['archive_bytes'] or archive_hash != manifest['archive_sha256']:
    raise SystemExit(f'compressed asset differs from pinned manifest: {archive}')
pathlib.Path(str(name) + '.sha256').write_text(f'{raw_hash}  {name.name}\n')
pathlib.Path(str(archive) + '.sha256').write_text(f'{archive_hash}  {archive.name}\n')
PY
cp "$(dirname "$file")/provenance.json" "$provenance"
gh release upload "$tag" "$archive" "$file.sha256" "$archive.sha256" "$provenance" --repo cole-brokamp/s2-tracts --clobber
