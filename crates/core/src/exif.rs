use std::io;
use std::panic;
use std::path::Path;

use jiff::civil::DateTime;
use jiff::fmt::strtime;
use jiff::tz::Offset;
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use little_exif::rational::uR64;

/// The tags lapsewright shows and carries into the video. `None` = absent or unusable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExifData {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    /// Wall-clock time of the shot, as the camera wrote it.
    pub taken_at: Option<DateTime>,
    /// UTC offset of `taken_at`; many cameras leave it out.
    pub offset: Option<Offset>,
    pub gps: Option<Gps>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
}

/// Decimal degrees; south and west are negative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gps {
    pub lat: f64,
    pub lon: f64,
}

/// A file without EXIF reads as `ExifData::default()`; only failing to read the file is an error.
pub fn read(path: &Path) -> io::Result<ExifData> {
    // The parser walks offsets taken from the file; a malformed photo must not take the app down.
    let metadata = match panic::catch_unwind(|| Metadata::new_from_path(path)) {
        Ok(Ok(metadata)) => metadata,
        // little_exif reports "no EXIF" and unparsable EXIF alike as `Other`: nothing to carry over.
        Ok(Err(e)) if e.kind() == io::ErrorKind::Other => return Ok(ExifData::default()),
        Ok(Err(e)) => return Err(e),
        Err(_) => return Ok(ExifData::default()),
    };
    let text = |tag| text(&metadata, tag);
    Ok(ExifData {
        make: text(ExifTag::Make(String::new())),
        model: text(ExifTag::Model(String::new())),
        lens: text(ExifTag::LensModel(String::new())),
        taken_at: text(ExifTag::DateTimeOriginal(String::new()))
            .and_then(|s| DateTime::strptime("%Y:%m:%d %H:%M:%S", s).ok()),
        offset: text(ExifTag::OffsetTimeOriginal(String::new()))
            .and_then(|s| strtime::parse("%:z", s).ok()?.offset()),
        gps: gps(&metadata),
        artist: text(ExifTag::Artist(String::new())),
        copyright: text(ExifTag::Copyright(String::new())),
    })
}

/// Trimmed, without control characters (cameras pad with NULs and spaces); empty = absent.
fn text(metadata: &Metadata, tag: ExifTag) -> Option<String> {
    let raw = metadata.get_tag(&tag).find_map(|found| match found {
        ExifTag::Make(s)
        | ExifTag::Model(s)
        | ExifTag::LensModel(s)
        | ExifTag::DateTimeOriginal(s)
        | ExifTag::OffsetTimeOriginal(s)
        | ExifTag::GPSLatitudeRef(s)
        | ExifTag::GPSLongitudeRef(s)
        | ExifTag::Artist(s)
        | ExifTag::Copyright(s) => Some(s),
        _ => None,
    })?;
    let clean: String = raw.chars().filter(|c| !c.is_control()).collect();
    Some(clean.trim().to_string()).filter(|s| !s.is_empty())
}

fn rationals(metadata: &Metadata, tag: ExifTag) -> Option<&[uR64]> {
    metadata.get_tag(&tag).find_map(|found| match found {
        ExifTag::GPSLatitude(v) | ExifTag::GPSLongitude(v) => Some(v.as_slice()),
        _ => None,
    })
}

fn gps(metadata: &Metadata) -> Option<Gps> {
    let lat_ref = text(metadata, ExifTag::GPSLatitudeRef(String::new()))?;
    let lon_ref = text(metadata, ExifTag::GPSLongitudeRef(String::new()))?;
    let lat = rationals(metadata, ExifTag::GPSLatitude(Vec::new()))?;
    let lon = rationals(metadata, ExifTag::GPSLongitude(Vec::new()))?;
    Some(Gps { lat: coordinate(lat, &lat_ref, ("N", "S"), 90.0)?, lon: coordinate(lon, &lon_ref, ("E", "W"), 180.0)? })
}

/// Degrees, minutes, seconds plus a hemisphere letter. Without a valid letter the sign is
/// unknown, and a mirrored location is worse than none.
fn coordinate(dms: &[uR64], hemisphere: &str, (positive, negative): (&str, &str), limit: f64) -> Option<f64> {
    let [d, m, s] = dms else { return None };
    let part = |r: &uR64| (r.denominator != 0).then(|| f64::from(r.nominator) / f64::from(r.denominator));
    let value = part(d)? + part(m)? / 60.0 + part(s)? / 3600.0;
    if value > limit {
        return None;
    }
    match hemisphere {
        h if h == positive => Some(value),
        h if h == negative => Some(-value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dms(d: u32, m: u32, s: (u32, u32)) -> [uR64; 3] {
        let r = |nominator, denominator| uR64 { nominator, denominator };
        [r(d, 1), r(m, 1), r(s.0, s.1)]
    }

    const NS: (&str, &str) = ("N", "S");

    #[test]
    fn southern_hemisphere_is_negative() {
        let value = coordinate(&dms(33, 51, (2448, 100)), "S", NS, 90.0).unwrap();
        assert!((value + 33.8568).abs() < 1e-9);
    }

    #[test]
    fn unusable_coordinates_are_dropped() {
        assert_eq!(coordinate(&dms(37, 33, (594, 0)), "N", NS, 90.0), None, "zero denominator");
        assert_eq!(coordinate(&dms(91, 0, (0, 1)), "N", NS, 90.0), None, "out of range");
        assert_eq!(coordinate(&dms(37, 33, (594, 10)), "", NS, 90.0), None, "no hemisphere");
        assert_eq!(coordinate(&dms(37, 33, (594, 10))[..2], "N", NS, 90.0), None, "two parts");
    }
}
