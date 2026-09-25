#!/usr/bin/env python3
"""Verify and compress every prepared vintage, then pin both hashes in the CLI."""

import hashlib
import json
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parent.parent
output = pathlib.Path(__file__).resolve().parent / "assets.lock.json"


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


assets = {}
for year in range(2010, 2026):
    folder = root / "dist" / f"census-tracts-{year}-r1"
    if not folder.exists() and year in (2010, 2020):
        folder = root / "dist" / "census-tracts-2010-2020-r1-r"
    raw = folder / f"tracts_{year}.fgb"
    provenance = json.loads((folder / "provenance.json").read_text())
    record = next(item for item in provenance["years"] if item["year"] == year)
    raw_bytes, raw_sha = raw.stat().st_size, digest(raw)
    if raw_bytes != record["bytes"] or raw_sha != record["sha256"]:
        raise SystemExit(f"prepared file differs from provenance: {raw}")
    archive = pathlib.Path(str(raw) + ".zst")
    subprocess.run(["zstd", "-9", "--force", "--quiet", str(raw), "-o", str(archive)], check=True)
    subprocess.run(["zstd", "--test", "--quiet", str(archive)], check=True)
    assets[str(year)] = {
        "raw_bytes": raw_bytes,
        "raw_sha256": raw_sha,
        "archive_bytes": archive.stat().st_size,
        "archive_sha256": digest(archive),
    }
    print(year, raw_bytes, assets[str(year)]["archive_bytes"], flush=True)

content = json.dumps(assets, indent=2) + "\n"
if output.exists() and output.read_text() != content:
    raise SystemExit(f"asset manifest differs; inspect before replacing {output}")
output.write_text(content)
print(f"Pinned {output}")
