use super::*;
use flatgeobuf::{FgbWriter, FgbWriterOptions};
use geo::{Geometry, polygon};
use geozero::{ColumnValue, PropertyProcessor};
use std::io::{Cursor, SeekFrom};

fn rectangle(x: f64, y: f64, radius: f64) -> Polygon {
    polygon![(x:x-radius,y:y-radius),(x:x+radius,y:y-radius),
             (x:x+radius,y:y+radius),(x:x-radius,y:y+radius),(x:x-radius,y:y-radius)]
}

fn fixture_bytes(year: u16, features: &[(String, Polygon)]) -> Vec<u8> {
    let mut writer = FgbWriter::create_with_options(
        &format!("tracts_{year}"),
        GeometryType::Polygon,
        FgbWriterOptions {
            title: Some(BUNDLE_TITLE),
            ..Default::default()
        },
    )
    .unwrap();
    writer.add_column("GEOID", ColumnType::String, |_, _| {});
    for (geoid, polygon) in features {
        writer
            .add_feature_geom(Geometry::Polygon(polygon.clone()), |f| {
                f.property(0, "GEOID", &ColumnValue::String(geoid)).unwrap();
            })
            .unwrap();
    }
    let mut bytes = Vec::new();
    writer.write(&mut bytes).unwrap();
    bytes
}

fn bundle(a: &[(String, Polygon)], b: &[(String, Polygon)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (year, features) in [(2010, a), (2020, b)] {
        std::fs::write(
            dir.path().join(format!("tracts_{year}.fgb")),
            fixture_bytes(year, features),
        )
        .unwrap();
    }
    dir
}

fn id_and_center(lat: f64, lng: f64) -> (u64, Coord) {
    let id = CellID::from(LatLng::from_degrees(lat, lng));
    let ll = LatLng::from(id);
    (
        id.0,
        Coord {
            x: ll.lng.deg(),
            y: ll.lat.deg(),
        },
    )
}

#[test]
fn inputs_order_duplicates_independent_years_and_reuse() {
    let (a, pa) = id_and_center(10., 10.);
    let (b, pb) = id_and_center(20., 20.);
    let dir = bundle(
        &[("01001000100".into(), rectangle(pa.x, pa.y, 0.1))],
        &[("09001000200".into(), rectangle(pb.x, pb.y, 0.1))],
    );
    let mut lookup = TractLookup::open(dir.path()).unwrap();
    assert_eq!(lookup.lookup(&[]).unwrap(), vec![]);
    let ra = TractIds {
        tract_2010: Some("01001000100".into()),
        tract_2020: None,
    };
    let rb = TractIds {
        tract_2010: None,
        tract_2020: Some("09001000200".into()),
    };
    for _ in 0..3 {
        assert_eq!(
            lookup.lookup(&[b, a, a, b]).unwrap(),
            vec![rb.clone(), ra.clone(), ra.clone(), rb.clone()]
        );
    }
    for value in [0, u64::MAX, CellID(a).parent(29).0, CellID::from_face(0).0] {
        assert!(matches!(lookup.lookup(&[a, value, b]),
            Err(LookupError::InvalidCellId { index: 1, value: v }) if v == value));
    }
    // Whole-batch validation must take precedence over dataset access.
    std::fs::OpenOptions::new()
        .write(true)
        .open(dir.path().join("tracts_2010.fgb"))
        .unwrap()
        .set_len(0)
        .unwrap();
    assert!(matches!(
        lookup.lookup(&[a, 0]),
        Err(LookupError::InvalidCellId { index: 1, value: 0 })
    ));
    assert!(lookup.lookup(&[a]).is_err());
    assert!(lookup.lookup(&[]).unwrap().is_empty());
}

fn point_result(features: Vec<(String, Polygon)>, point: Coord) -> Option<String> {
    let bytes = fixture_bytes(2010, &features);
    query(&mut Cursor::new(bytes), [-360., -90., 360., 90.], point).unwrap()
}

#[test]
fn complete_polygon_predicates() {
    let geoid = "01001000100".to_owned();
    let polygon = Polygon::new(
        rectangle(0., 0., 4.).exterior().clone(),
        vec![rectangle(0., 0., 1.).exterior().clone()],
    );
    for (x, y, expected) in [
        (2., 2., Some(geoid.clone())),
        (5., 0., None),
        (4., 0., None),
        (4., 4., None),
        (0., 0., None),
        (1., 0., None),
        (1., 1., None),
    ] {
        assert_eq!(
            point_result(vec![(geoid.clone(), polygon.clone())], Coord { x, y }),
            expected
        );
    }
}

#[test]
fn ambiguity_boundary_precedence_and_same_geoid_components() {
    let p = Coord { x: 0., y: 0. };
    let a = ("01001000100".into(), rectangle(0., 0., 2.));
    let b = ("01001000200".into(), rectangle(0., 0., 1.));
    let boundary = ("01001000300".into(), rectangle(1., 0., 1.));
    for features in [
        vec![a.clone(), b.clone()],
        vec![b.clone(), a.clone()],
        vec![a.clone(), boundary.clone()],
        vec![boundary, a.clone()],
    ] {
        assert_eq!(point_result(features, p), None);
    }
    assert_eq!(
        point_result(vec![a.clone(), a.clone()], p),
        Some(a.0.clone())
    );
    assert_eq!(
        point_result(vec![a.clone(), (a.0.clone(), rectangle(10., 10., 1.))], p),
        Some(a.0)
    );
}

#[test]
fn exact_s2_center_on_edge_and_vertex() {
    let (id, p) = id_and_center(38., -84.);
    for polygon in [
        polygon![(x:p.x,y:p.y-1.),(x:p.x+1.,y:p.y-1.),(x:p.x+1.,y:p.y+1.),(x:p.x,y:p.y+1.),(x:p.x,y:p.y-1.)],
        polygon![(x:p.x,y:p.y),(x:p.x+1.,y:p.y),(x:p.x+1.,y:p.y+1.),(x:p.x,y:p.y+1.),(x:p.x,y:p.y)],
    ] {
        let features = [("01001000100".into(), polygon)];
        let dir = bundle(&features, &features);
        assert_eq!(
            TractLookup::open(dir.path())
                .unwrap()
                .lookup(&[id])
                .unwrap(),
            vec![TractIds {
                tract_2010: None,
                tract_2020: None
            }]
        );
    }
}

#[test]
fn antimeridian_equivalent_longitudes_holes_and_boundary() {
    let geoid = "02016000100".to_owned();
    let poly = Polygon::new(
        rectangle(-180., 10., 2.).exterior().clone(),
        vec![rectangle(-180., 10., 0.5).exterior().clone()],
    );
    let features = vec![(geoid.clone(), poly)];
    for lng in [-181., 179.] {
        assert_eq!(
            point_result(features.clone(), Coord { x: lng, y: 10. }),
            Some(geoid.clone())
        );
    }
    assert_eq!(
        point_result(features.clone(), Coord { x: 180., y: 10. }),
        None
    );
    assert_eq!(point_result(features, Coord { x: 178., y: 10. }), None);
    // Same tract in two longitude representations still counts once.
    assert_eq!(
        point_result(
            vec![
                (geoid.clone(), rectangle(-181., 10., 0.1)),
                (geoid.clone(), rectangle(179., 10., 0.1))
            ],
            Coord { x: 179., y: 10. }
        ),
        Some(geoid)
    );
}

#[test]
fn missing_malformed_swapped_and_bad_properties() {
    let dir = tempfile::tempdir().unwrap();
    assert!(TractLookup::open(dir.path()).is_err());
    std::fs::write(dir.path().join("tracts_2010.fgb"), b"not flatgeobuf").unwrap();
    assert!(TractLookup::open(dir.path()).is_err());
    let a = [("01001000100".into(), rectangle(0., 0., 1.))];
    let dir = bundle(&a, &a);
    std::fs::copy(
        dir.path().join("tracts_2020.fgb"),
        dir.path().join("tracts_2010.fgb"),
    )
    .unwrap();
    assert!(TractLookup::open(dir.path()).is_err());
    for invalid in ["1001000100", "0100100010x", "", "010010001000"] {
        let bytes = fixture_bytes(2010, &[(invalid.into(), rectangle(0., 0., 1.))]);
        assert!(
            query(
                &mut Cursor::new(bytes),
                [-2., -2., 2., 2.],
                Coord { x: 0., y: 0. }
            )
            .is_err()
        );
    }
}

#[test]
fn actual_io_failure_is_not_an_unmatched_result() {
    struct FailRead(Cursor<Vec<u8>>);
    impl Read for FailRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected disk failure"))
        }
    }
    impl Seek for FailRead {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.0.seek(pos)
        }
    }
    assert!(
        query(
            &mut FailRead(Cursor::new(vec![])),
            [-2., -2., 2., 2.],
            Coord { x: 0., y: 0. }
        )
        .is_err()
    );
    let bytes = fixture_bytes(2010, &[("01001000100".into(), rectangle(0., 0., 1.))]);
    // Selected feature body/prefix truncation must fail, including upstream EOF behavior.
    for remove in [1, 4, 40, 100] {
        let mut truncated = bytes.clone();
        truncated.truncate(truncated.len() - remove);
        assert!(
            query(
                &mut Cursor::new(truncated),
                [-2., -2., 2., 2.],
                Coord { x: 0., y: 0. }
            )
            .is_err()
        );
    }
}

#[test]
fn incompatible_index_revision_and_schema_fail_at_open() {
    for (indexed, title, column) in [
        (false, Some(BUNDLE_TITLE), "GEOID"),
        (true, None, "GEOID"),
        (true, Some("census-tracts-2010-2020-r2"), "GEOID"),
        (true, Some(BUNDLE_TITLE), "GEOID10"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = FgbWriter::create_with_options(
            "tracts_2010",
            GeometryType::Polygon,
            FgbWriterOptions {
                write_index: indexed,
                title,
                ..Default::default()
            },
        )
        .unwrap();
        writer.add_column(column, ColumnType::String, |_, _| {});
        writer
            .add_feature_geom(Geometry::Polygon(rectangle(0., 0., 1.)), |f| {
                f.property(0, column, &ColumnValue::String("01001000100"))
                    .unwrap();
            })
            .unwrap();
        writer
            .write(File::create(dir.path().join("tracts_2010.fgb")).unwrap())
            .unwrap();
        assert!(Dataset::open(dir.path(), 2010).is_err());
    }
}

#[test]
fn boundary_of_same_geoid_also_wins() {
    assert_eq!(
        point_result(
            vec![
                ("01001000100".into(), rectangle(0., 0., 2.)),
                ("01001000100".into(), rectangle(1., 0., 1.)),
            ],
            Coord { x: 0., y: 0. }
        ),
        None
    );
}

#[test]
fn unwrapped_antimeridian_through_public_s2_api() {
    let (east, _) = id_and_center(10., 179.5);
    let (west, _) = id_and_center(10., -179.5);
    let features = [("02016000100".into(), rectangle(-180., 10., 1.))];
    let dir = bundle(&features, &features);
    let result = TractLookup::open(dir.path())
        .unwrap()
        .lookup(&[east, west, east])
        .unwrap();
    assert_eq!(
        result,
        vec![
            TractIds {
                tract_2010: Some("02016000100".into()),
                tract_2020: Some("02016000100".into())
            };
            3
        ]
    );
}
