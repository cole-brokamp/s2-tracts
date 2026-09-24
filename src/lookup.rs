//! Offline lookup of the fixed 2010 and 2020 decennial Census tracts.
//!
//! Only level-30 S2 cell IDs are accepted. Their centers are compared directly
//! with longitude/latitude coordinates using planar, strict containment.
//! No datum transformation, tolerance, nearest assignment, or network access.

use flatgeobuf::{ColumnType, FallibleStreamingIterator, FgbFeature, FgbReader, GeometryType};
use geo::algorithm::coordinate_position::{CoordPos, CoordinatePosition};
use geo::{Coord, LineString, Polygon};
use s2::{cellid::CellID, latlng::LatLng};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

const BUNDLE_TITLE: &str = "census-tracts-2010-2020-r1";

/// An input or dataset failure. Input errors identify the original batch index.
#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("invalid level-30 S2 cell ID at index {index}: {value}")]
    InvalidCellId { index: usize, value: u64 },
    #[error("dataset {path}: {message}")]
    Dataset { path: PathBuf, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TractIds {
    pub tract_2010: Option<String>,
    pub tract_2020: Option<String>,
}

/// Two reusable buffered readers; the national spatial indexes stay on disk.
pub struct TractLookup {
    tracts_2010: Dataset,
    tracts_2020: Dataset,
}

impl TractLookup {
    /// Open the extracted, prepared r1 bundle. Header/schema checks happen here;
    /// queried features are checked while reading. No checksum scans or downloads.
    pub fn open(bundle_dir: impl AsRef<Path>) -> Result<Self, LookupError> {
        Ok(Self {
            tracts_2010: Dataset::open(bundle_dir.as_ref(), 2010)?,
            tracts_2020: Dataset::open(bundle_dir.as_ref(), 2020)?,
        })
    }

    /// Validate the whole batch, then query both vintages for each unique center.
    /// Boundaries, ambiguous containment, and outside points all return `None`.
    pub fn lookup(&mut self, cell_ids: &[u64]) -> Result<Vec<TractIds>, LookupError> {
        for (index, &value) in cell_ids.iter().enumerate() {
            let id = CellID(value);
            if !id.is_valid() || !id.is_leaf() {
                return Err(LookupError::InvalidCellId { index, value });
            }
        }
        if cell_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.tracts_2010.check_length()?;
        self.tracts_2020.check_length()?;
        let mut unique = HashMap::new();
        for &id in cell_ids {
            if let std::collections::hash_map::Entry::Vacant(entry) = unique.entry(id) {
                let center = LatLng::from(CellID(id));
                let point = Coord {
                    x: center.lng.deg(),
                    y: center.lat.deg(),
                };
                entry.insert(TractIds {
                    tract_2010: self.tracts_2010.lookup(point)?,
                    tract_2020: self.tracts_2020.lookup(point)?,
                });
            }
        }
        Ok(cell_ids.iter().map(|id| unique[id].clone()).collect())
    }
}

struct Dataset {
    reader: BufReader<File>,
    path: PathBuf,
    length: u64,
    envelope: [f64; 4],
}

fn dataset_error(path: &Path, message: impl ToString) -> LookupError {
    LookupError::Dataset {
        path: path.to_owned(),
        message: message.to_string(),
    }
}

impl Dataset {
    fn open(dir: &Path, year: u16) -> Result<Self, LookupError> {
        let path = dir.join(format!("tracts_{year}.fgb"));
        let load = || -> Result<Self, Box<dyn std::error::Error>> {
            let file = File::open(&path)?;
            let length = file.metadata()?.len();
            let mut reader = BufReader::new(file);
            let fgb = FgbReader::open(&mut reader)?;
            let h = fgb.header();
            if h.name() != Some(format!("tracts_{year}").as_str())
                || h.title() != Some(BUNDLE_TITLE)
                || h.geometry_type() != GeometryType::Polygon
                || h.has_z() || h.has_m() || h.has_t() || h.has_tm()
                // flatgeobuf 6.0.1's seekable reader assumes node size 16.
                || h.index_node_size() != 16 || h.features_count() == 0
            {
                return Err(
                    "incompatible vintage, bundle revision, geometry, or spatial index".into(),
                );
            }
            let columns = h.columns().ok_or("missing GEOID column")?;
            if columns.len() != 1
                || columns.get(0).name() != "GEOID"
                || columns.get(0).type_() != ColumnType::String
            {
                return Err("expected exactly one string column named GEOID".into());
            }
            let bounds = h.envelope().ok_or("missing envelope")?;
            if bounds.len() != 4 {
                return Err("invalid envelope".into());
            }
            let envelope = [bounds.get(0), bounds.get(1), bounds.get(2), bounds.get(3)];
            if !envelope.iter().all(|v| v.is_finite())
                || envelope[0] > envelope[2]
                || envelope[1] > envelope[3]
                || envelope[0] < -360.0
                || envelope[2] > 360.0
                || envelope[1] < -90.0
                || envelope[3] > 90.0
            {
                return Err("incompatible unwrapped longitude/latitude envelope".into());
            }
            // Bound count before calculating the index size (including on corrupt headers).
            if h.features_count() > length / 40 {
                return Err("truncated index or invalid feature count".into());
            }
            let index_size =
                flatgeobuf::packed_r_tree::PackedRTree::index_size(h.features_count() as usize, 16)
                    as u64;
            drop(fgb);
            if reader.stream_position()? + index_size + 8 > length {
                return Err("truncated dataset".into());
            }
            Ok(Self {
                reader,
                path: path.clone(),
                length,
                envelope,
            })
        };
        load().map_err(|e| dataset_error(&path, e))
    }

    fn check_length(&self) -> Result<(), LookupError> {
        let len = self
            .reader
            .get_ref()
            .metadata()
            .map_err(|e| dataset_error(&self.path, e))?
            .len();
        if len != self.length {
            return Err(dataset_error(&self.path, "dataset size changed after open"));
        }
        Ok(())
    }

    fn lookup(&mut self, point: Coord) -> Result<Option<String>, LookupError> {
        query(&mut self.reader, self.envelope, point).map_err(|e| dataset_error(&self.path, e))
    }
}

// Generic reader makes genuine I/O failure injection possible in offline tests.
fn query(
    reader: &mut (impl Read + Seek),
    envelope: [f64; 4],
    point: Coord,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let mut matches = HashSet::new();
    let mut boundary = false;
    // Preparation places exterior midpoint in [-180, 180), with width <180.
    // These three representations therefore cover every stored component.
    for shift in [-360.0, 0.0, 360.0] {
        let p = Coord {
            x: point.x + shift,
            y: point.y,
        };
        if p.x < envelope[0] || p.x > envelope[2] || p.y < envelope[1] || p.y > envelope[3] {
            continue;
        }
        reader.rewind()?;
        let mut candidates = FgbReader::open(&mut *reader)?.select_bbox(p.x, p.y, p.x, p.y)?;
        let expected = candidates
            .features_count()
            .ok_or("missing candidate count")?;
        let mut seen = 0;
        while let Some(feature) = candidates.next()? {
            seen += 1;
            let (geoid, polygon) = decode_feature(feature)?;
            match polygon.coordinate_position(&p) {
                CoordPos::OnBoundary => boundary = true,
                CoordPos::Inside => {
                    matches.insert(geoid);
                }
                CoordPos::Outside => {}
            }
        }
        // The upstream iterator treats a failed size-prefix read as EOF.
        // A selected feature must never silently disappear on an I/O failure.
        if seen != expected {
            return Err("short read in selected features".into());
        }
    }
    Ok(if !boundary && matches.len() == 1 {
        matches.into_iter().next()
    } else {
        None
    })
}

fn decode_feature(feature: &FgbFeature) -> Result<(String, Polygon), Box<dyn std::error::Error>> {
    // A single string property is encoded as u16 column + u32 length + bytes.
    // Check it directly so malformed property offsets cannot panic upstream.
    let props = feature.fbs_feature().properties().ok_or("missing GEOID")?;
    let bytes = props.bytes();
    if bytes.len() != 17
        || bytes[..6] != [0, 0, 11, 0, 0, 0]
        || !bytes[6..].iter().all(u8::is_ascii_digit)
    {
        return Err("expected an 11-digit GEOID string".into());
    }
    let geoid = std::str::from_utf8(&bytes[6..])?.to_owned();
    let geometry = feature.geometry().ok_or("missing polygon geometry")?;
    if geometry.parts().is_some()
        || geometry.z().is_some()
        || geometry.m().is_some()
        || geometry.t().is_some()
        || geometry.tm().is_some()
    {
        return Err("expected a 2D polygon component".into());
    }
    let xy = geometry.xy().ok_or("missing polygon coordinates")?;
    if xy.len() % 2 != 0 {
        return Err("odd coordinate array length".into());
    }
    let count = xy.len() / 2;
    let ends: Vec<usize> = geometry
        .ends()
        .map(|e| e.iter().map(|n| n as usize).collect())
        .filter(|e: &Vec<_>| !e.is_empty())
        .unwrap_or_else(|| vec![count]);
    if ends.last() != Some(&count) {
        return Err("ring ends do not cover coordinates".into());
    }
    let mut start = 0;
    let mut rings = Vec::with_capacity(ends.len());
    for end in ends {
        if end > count || end < start + 4 {
            return Err("invalid ring offsets".into());
        }
        let mut ring = Vec::with_capacity(end - start);
        for i in start..end {
            let c = Coord {
                x: xy.get(i * 2),
                y: xy.get(i * 2 + 1),
            };
            if !c.x.is_finite() || !c.y.is_finite() || c.x.abs() > 360.0 || c.y.abs() > 90.0 {
                return Err("invalid longitude/latitude coordinate".into());
            }
            ring.push(c);
        }
        if ring.first() != ring.last() {
            return Err("unclosed ring".into());
        }
        if ring.windows(2).any(|w| (w[0].x - w[1].x).abs() > 180.0) {
            return Err("polygon was not unwrapped during preparation".into());
        }
        rings.push(LineString::new(ring));
        start = end;
    }
    let mut rings = rings.into_iter();
    Ok((
        geoid,
        Polygon::new(rings.next().ok_or("empty polygon")?, rings.collect()),
    ))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
