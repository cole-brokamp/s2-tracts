//! Maintainer-only TIGER/Line to indexed FlatGeobuf converter.
//! GDAL reads zipped shapefiles; this program owns the coordinate policy.
use flatgeobuf::{ColumnType, FgbWriter, FgbWriterOptions, GeometryType};
use geo::algorithm::Area;
use geo::{Coord, Geometry, LineString, Polygon};
use geozero::{ColumnValue, PropertyProcessor};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const STATES: &str = "01 02 04 05 06 08 09 10 11 12 13 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 44 45 46 47 48 49 50 51 53 54 55 56 60 66 69 72 78";
const POLICY: &str = "Source longitude/latitude without datum transformation; integer-360 longitude unwrapping and hole alignment only; no repair, simplification, clipping, or rounding.";

fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, Box<dyn Error>> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| fail(format!("missing {key}")))
}

fn command_output(program: &str, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(fail(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn sha256(path: &Path) -> Result<String, Box<dyn Error>> {
    let name = path.to_str().ok_or("non-UTF-8 path")?;
    let output = if cfg!(target_os = "macos") {
        command_output("shasum", &["-a", "256", name])?
    } else {
        command_output("sha256sum", &[name])?
    };
    Ok(output
        .split_whitespace()
        .next()
        .ok_or("empty checksum")?
        .to_owned())
}

fn coord(value: &Value) -> Result<Coord, Box<dyn Error>> {
    let p = value.as_array().ok_or("coordinate must be an array")?;
    if p.len() != 2 {
        return Err(fail("source geometry must be 2D"));
    }
    let x = p[0].as_f64().ok_or("invalid longitude")?;
    let y = p[1].as_f64().ok_or("invalid latitude")?;
    if !x.is_finite() || !y.is_finite() || x.abs() > 180.0 || y.abs() > 90.0 {
        return Err(fail("invalid source longitude/latitude"));
    }
    Ok(Coord { x, y })
}

fn ring(value: &Value) -> Result<LineString, Box<dyn Error>> {
    let points = value.as_array().ok_or("ring must be an array")?;
    let mut coords = points.iter().map(coord).collect::<Result<Vec<_>, _>>()?;
    if coords.len() < 4 || coords.first() != coords.last() {
        return Err(fail("unclosed or short ring"));
    }
    let mut shift = 0.0;
    for i in 1..coords.len() {
        let x = coords[i].x;
        while x + shift - coords[i - 1].x > 180.0 {
            shift -= 360.0;
        }
        while x + shift - coords[i - 1].x < -180.0 {
            shift += 360.0;
        }
        coords[i].x = x + shift;
    }
    if coords.first() != coords.last() {
        return Err(fail("ring winds around the globe"));
    }
    Ok(LineString::new(coords))
}

fn midpoint(ring: &LineString) -> f64 {
    let min = ring.0.iter().map(|c| c.x).fold(f64::INFINITY, f64::min);
    let max = ring.0.iter().map(|c| c.x).fold(f64::NEG_INFINITY, f64::max);
    (min + max) / 2.0
}

fn shift(ring: &mut LineString, offset: f64) {
    for point in &mut ring.0 {
        point.x += offset;
    }
}

fn polygon(value: &Value) -> Result<Polygon, Box<dyn Error>> {
    let rings = value.as_array().ok_or("polygon must be an array")?;
    if rings.is_empty() {
        return Err(fail("empty polygon"));
    }
    let mut exterior = ring(&rings[0])?;
    let offset = -360.0 * ((midpoint(&exterior) + 180.0) / 360.0).floor();
    shift(&mut exterior, offset);
    let min = exterior.0.iter().map(|c| c.x).fold(f64::INFINITY, f64::min);
    let max = exterior
        .0
        .iter()
        .map(|c| c.x)
        .fold(f64::NEG_INFINITY, f64::max);
    if max - min >= 180.0 {
        return Err(fail("polygon too wide for lookup"));
    }
    let middle = midpoint(&exterior);
    let mut holes = Vec::new();
    for value in &rings[1..] {
        let mut hole = ring(value)?;
        let offset = 360.0 * ((middle - midpoint(&hole)) / 360.0).round();
        shift(&mut hole, offset);
        holes.push(hole);
    }
    let polygon = Polygon::new(exterior, holes);
    if polygon.unsigned_area() <= 0.0 {
        return Err(fail("zero-area normalized polygon"));
    }
    Ok(polygon)
}

fn components(geometry: &Value) -> Result<Vec<Polygon>, Box<dyn Error>> {
    let coords = geometry.get("coordinates").ok_or("missing coordinates")?;
    match text(geometry, "type")? {
        "Polygon" => Ok(vec![polygon(coords)?]),
        "MultiPolygon" => {
            let values = coords.as_array().ok_or("invalid multipolygon")?;
            if values.is_empty() {
                return Err(fail("empty multipolygon"));
            }
            values.iter().map(polygon).collect()
        }
        _ => Err(fail("non-polygon source geometry")),
    }
}

fn accept_geoid(
    geoid: &str,
    state: &str,
    seen: &mut HashSet<String>,
) -> Result<(), Box<dyn Error>> {
    if geoid.len() != 11
        || !geoid.bytes().all(|b| b.is_ascii_digit())
        || !geoid.starts_with(state)
        || !seen.insert(geoid.to_owned())
    {
        return Err(fail(format!("invalid or duplicate GEOID: {geoid}")));
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 6 || args[0] != "--vintage" || args[2] != "--sources" || args[4] != "--output"
    {
        return Err(fail(
            "usage: prepare-tracts --vintage YEAR --sources DIR --output DIR",
        ));
    }
    let year: u16 = args[1].parse()?;
    if !(2010..=2025).contains(&year) {
        return Err(fail("vintage must be 2010 through 2025"));
    }
    let sources = Path::new(&args[3]);
    let output = Path::new(&args[5]);
    let staging = PathBuf::from(format!("{}.building", output.display()));
    if output.exists() || staging.exists() {
        return Err(fail("output or staging directory already exists"));
    }
    let lock: Value =
        serde_json::from_reader(File::open(format!("scripts/sources-{year}.lock.json"))?)?;
    let records = lock.as_array().ok_or("source lock must be an array")?;
    if records.len() != 56 {
        return Err(fail("source lock must contain 56 archives"));
    }
    fs::create_dir_all(&staging)?;
    let title = format!("census-tracts-{year}-r2");
    let name = format!("tracts_{year}");
    let mut writer = FgbWriter::create_with_options(
        &name,
        GeometryType::Polygon,
        FgbWriterOptions {
            title: Some(&title),
            ..Default::default()
        },
    )?;
    writer.add_column("GEOID", ColumnType::String, |_, _| {});
    let mut seen = HashSet::new();
    let mut source_info = Vec::new();
    let mut component_total = 0usize;
    for (record, state) in records.iter().zip(STATES.split_whitespace()) {
        if record.get("year").and_then(Value::as_u64) != Some(year as u64)
            || text(record, "state")? != state
        {
            return Err(fail("source lock has missing or unordered state"));
        }
        let filename = text(record, "filename")?;
        let expected = format!(
            "tl_{year}_{state}_tract{}.zip",
            if year == 2010 { "10" } else { "" }
        );
        if filename != expected {
            return Err(fail("source lock filename mismatch"));
        }
        let path = sources.join(filename).canonicalize()?;
        if fs::metadata(&path)?.len()
            != record["bytes"]
                .as_u64()
                .ok_or("invalid source byte count")?
            || sha256(&path)? != text(record, "sha256")?
        {
            return Err(fail(format!(
                "source differs from lock: {}",
                path.display()
            )));
        }
        let vsi = format!("/vsizip/{}", path.display());
        let layer = filename.trim_end_matches(".zip");
        let metadata: Value =
            serde_json::from_str(&command_output("ogrinfo", &["-json", "-so", &vsi, layer])?)?;
        let info = &metadata["layers"][0];
        let crs = text(&info["geometryFields"][0]["coordinateSystem"], "wkt")?;
        if !crs.starts_with("GEOGCRS[") {
            return Err(fail("source CRS is not geographic"));
        }
        let field = if year == 2010 { "GEOID10" } else { "GEOID" };
        // GeoJSONSeq otherwise silently transforms NAD83 to WGS84. Override both
        // sides with the same CRS so serialization preserves source coordinate numbers.
        let mut child = Command::new("ogr2ogr")
            .args([
                "-f",
                "GeoJSONSeq",
                "/vsistdout/",
                &vsi,
                "-s_srs",
                "EPSG:4326",
                "-t_srs",
                "EPSG:4326",
                "-lco",
                "RS=NO",
                "-lco",
                "COORDINATE_PRECISION=17",
                "-select",
                field,
            ])
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().ok_or("missing GDAL output")?;
        let mut state_tracts = 0usize;
        let mut state_components = 0usize;
        for line in BufReader::new(stdout).lines() {
            let feature: Value = serde_json::from_str(&line?)?;
            let geoid = text(&feature["properties"], field)?;
            accept_geoid(geoid, state, &mut seen)?;
            let polygons = components(&feature["geometry"])?;
            for shape in polygons {
                writer.add_feature_geom(Geometry::Polygon(shape), |f| {
                    f.property(0, "GEOID", &ColumnValue::String(geoid))
                        .expect("GEOID property");
                })?;
                state_components += 1;
            }
            state_tracts += 1;
        }
        if !child.wait()?.success() {
            return Err(fail(format!("GDAL failed for {filename}")));
        }
        if info["featureCount"].as_u64() != Some(state_tracts as u64) || state_tracts == 0 {
            return Err(fail(format!("source feature count mismatch: {filename}")));
        }
        component_total += state_components;
        let mut source = record.clone();
        let object = source.as_object_mut().ok_or("invalid source record")?;
        object.insert("crs".into(), json!(crs));
        object.insert("tracts".into(), json!(state_tracts));
        object.insert("components".into(), json!(state_components));
        source_info.push(source);
        eprintln!("{year} {state}: {state_tracts} tracts, {state_components} components");
    }
    let file = format!("{name}.fgb");
    let path = staging.join(&file);
    let mut out = BufWriter::new(File::create(&path)?);
    writer.write(&mut out)?;
    out.flush()?;
    drop(out);
    let control = format!(
        "SELECT ST_IsValid(GeomFromText('POLYGON((0 0,2 2,0 2,2 0,0 0))')) AS valid FROM {name} LIMIT 1"
    );
    let control_output = command_output(
        "ogrinfo",
        &[
            "-ro",
            "-dialect",
            "SQLite",
            "-sql",
            &control,
            path.to_str().ok_or("non-UTF-8 output path")?,
        ],
    )?;
    if !control_output
        .lines()
        .any(|line| line.trim() == "valid (Integer) = 0")
    {
        return Err(fail("GDAL must evaluate polygon validity with GEOS"));
    }
    let sql = format!("SELECT COUNT(*) AS bad FROM {name} WHERE ST_IsValid(geometry) = 0");
    let check = command_output(
        "ogrinfo",
        &[
            "-ro",
            "-dialect",
            "SQLite",
            "-sql",
            &sql,
            path.to_str().ok_or("non-UTF-8 output path")?,
        ],
    )?;
    if !check
        .lines()
        .any(|line| line.trim() == "bad (Integer) = 0" || line.trim() == "bad (Integer64) = 0")
    {
        return Err(fail(format!(
            "GEOS found invalid normalized geometry: {check}"
        )));
    }
    let provenance = json!({
        "bundle": title,
        "years": [{"year": year, "file": file, "tracts": seen.len(), "components": component_total,
            "bytes": fs::metadata(&path)?.len(), "sha256": sha256(&path)?, "sources": source_info}],
        "preparation": {
            "preparer": env!("CARGO_PKG_VERSION"),
            "rustc": command_output("rustc", &["--version"])?.trim(),
            "gdal": command_output("gdal-config", &["--version"])?.trim(),
            "geos": command_output("geos-config", &["--version"])?.trim()
        },
        "coordinate_policy": POLICY
    });
    serde_json::to_writer_pretty(File::create(staging.join("provenance.json"))?, &provenance)?;
    fs::rename(&staging, output)?;
    eprintln!("prepared {}", output.display());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("prepare-tracts: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::algorithm::Validation;
    #[test]
    fn geometry_rules() {
        let polygon = json!([
            [[179., 0.], [-179., 0.], [-179., 2.], [179., 2.], [179., 0.]],
            [
                [179.2, 0.2],
                [-179.2, 0.2],
                [-179.2, 0.8],
                [179.2, 0.8],
                [179.2, 0.2]
            ]
        ]);
        let normalized = super::polygon(&polygon).unwrap();
        assert!(normalized.is_valid());
        assert!(
            normalized
                .exterior()
                .0
                .windows(2)
                .all(|w| (w[0].x - w[1].x).abs() <= 180.)
        );
        assert_eq!(normalized.interiors().len(), 1);
        assert!(
            super::polygon(&json!([[[0., 0.], [1., 1.], [0., 1.], [1., 0.], [0., 0.]]])).is_err()
        );
        assert!(super::polygon(&json!([[[0., 0.], [1., 0.], [1., 1.]]])).is_err());
        assert_eq!(
            components(&json!({"type":"MultiPolygon","coordinates":[polygon.clone(),polygon]}))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn duplicate_and_malformed_geoids_fail() {
        let mut seen = HashSet::new();
        accept_geoid("01001000100", "01", &mut seen).unwrap();
        assert!(accept_geoid("01001000100", "01", &mut seen).is_err());
        assert!(accept_geoid("02001000100", "01", &mut seen).is_err());
        assert!(accept_geoid("01001abc100", "01", &mut seen).is_err());
    }
}
