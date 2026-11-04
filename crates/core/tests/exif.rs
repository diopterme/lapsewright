//! Fixtures in tests/fixtures: 64×48 frames from ffmpeg `testsrc2`, tags written by exiftool 13.55.
//! gps.jxl is `cjxl gps.jpg` (v0.12), which stores the EXIF Brotli-compressed in a `brob` box.

use std::path::{Path, PathBuf};

use jiff::civil::date;
use jiff::tz::offset;
use lapsewright_core::exif::{self, ExifData, Gps};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name)
}

fn sony_seoul() -> ExifData {
    ExifData {
        make: Some("SONY".into()),
        model: Some("ILCE-7SM3".into()),
        lens: Some("FE 24mm F1.4 GM".into()),
        taken_at: Some(date(2026, 6, 1).at(14, 22, 10, 0)),
        offset: Some(offset(9)),
        gps: Some(Gps { lat: 37.5665, lon: 126.978 }),
        artist: Some("Kim".into()),
        copyright: Some("Kim".into()),
    }
}

/// GPS goes through degree/minute/second rationals, so compare it with a tolerance.
fn assert_same(actual: ExifData, expected: ExifData) {
    let (a, e) = (actual.gps.expect("gps"), expected.gps.expect("gps"));
    assert!((a.lat - e.lat).abs() < 1e-9 && (a.lon - e.lon).abs() < 1e-9, "{a:?} != {e:?}");
    assert_eq!(ExifData { gps: None, ..actual }, ExifData { gps: None, ..expected });
}

#[test]
fn reads_jpg() {
    assert_same(exif::read(&fixture("gps.jpg")).unwrap(), sony_seoul());
}

#[test]
fn reads_jxl_with_compressed_exif() {
    assert_same(exif::read(&fixture("gps.jxl")).unwrap(), sony_seoul());
}

/// gps.tif: 16-bit like a raw converter's export, tags copied from gps.jpg.
#[test]
fn reads_tif() {
    assert_same(exif::read(&fixture("gps.tif")).unwrap(), sony_seoul());
}

#[test]
fn reads_png_with_western_longitude() {
    let expected = ExifData {
        make: Some("Canon".into()),
        model: Some("EOS R5".into()),
        taken_at: Some(date(2025, 12, 31).at(23, 59, 59, 0)),
        offset: Some(offset(-5)),
        gps: Some(Gps { lat: 40.6892, lon: -74.0445 }),
        ..ExifData::default()
    };
    assert_same(exif::read(&fixture("gps.png")).unwrap(), expected);
}

#[test]
fn file_without_exif_reads_as_empty() {
    assert_eq!(exif::read(&fixture("no-exif.png")).unwrap(), ExifData::default());
}

#[test]
fn missing_file_is_an_error() {
    assert!(exif::read(&fixture("missing.jpg")).is_err());
}
