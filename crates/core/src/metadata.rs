use jiff::tz::TimeZone;

use crate::exif::{ExifData, Gps};

/// What goes into the video. Measured with ffmpeg 9.0.2 and AVFoundation (what Photos and
/// QuickTime read).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VideoMetadata {
    /// `-metadata` pairs. MOV keeps all of them in `udta`, MP4 keeps only copyright, location
    /// and the time. `creation_time` lands in `mvhd` as UTC, so its offset is used, not kept.
    pub tags: Vec<(&'static str, String)>,
    /// `com.apple.quicktime.*` keys ffmpeg cannot place where Apple reads them; written by
    /// `quicktime::add_keys` after encoding. They keep the local time with its offset, and the lens.
    pub keys: Vec<(&'static str, String)>,
}

/// `gps` is the location to write (from EXIF, edited, or `None` once cleared). `author` and
/// `copyright_holder` come from the app settings and win over the photo's Artist and Copyright.
pub fn build(
    exif: &ExifData,
    gps: Option<Gps>,
    author: Option<&str>,
    copyright_holder: Option<&str>,
    local: &TimeZone,
    this_year: i16,
) -> VideoMetadata {
    let filled = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let author = filled(author).or_else(|| exif.artist.clone());
    let holder = filled(copyright_holder).or_else(|| exif.copyright.clone());
    let year = exif.taken_at.map_or(this_year, |t| t.year());
    // Without an offset the photo was most likely taken in the computer's zone; using the zone
    // rather than its current offset keeps summer photos right in winter.
    let tz = exif.offset.map_or_else(|| local.clone(), TimeZone::fixed);
    let taken = exif.taken_at.and_then(|t| t.to_zoned(tz).ok());
    // No altitude: the photo's is unknown and `+0` would claim sea level.
    // MP4 drops a `location` without a language; AVFoundation reads it back only from `location-eng`.
    let location = gps.map(|g| format!("{:+.6}{:+.6}/", g.lat, g.lon));
    let tags = present([
        ("make", exif.make.clone()),
        ("model", exif.model.clone()),
        ("creation_time", taken.as_ref().map(|z| z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string())),
        ("author", author),
        ("copyright", holder.map(|h| copyright(year, h))),
        ("location", location.clone()),
        ("location-eng", location),
    ]);
    let keys = present([
        // The form iPhone videos carry: offset without a colon.
        ("com.apple.quicktime.creationdate", taken.map(|z| z.strftime("%Y-%m-%dT%H:%M:%S%z").to_string())),
        // MP4 drops ffmpeg's make/model tags; these keys carry them in both containers.
        ("com.apple.quicktime.make", exif.make.clone()),
        ("com.apple.quicktime.model", exif.model.clone()),
        ("com.apple.quicktime.camera.lens_model", exif.lens.clone()),
    ]);
    VideoMetadata { tags, keys }
}

/// A holder already written out ("Copyright 2025 Kim", "© Kim") is kept as is, not wrapped twice.
pub fn copyright(year: i16, holder: String) -> String {
    let written_out = holder.starts_with('©') || holder.get(..9).is_some_and(|p| p.eq_ignore_ascii_case("copyright"));
    if written_out { holder } else { format!("Copyright {year} {holder}") }
}

fn present<const N: usize>(pairs: [(&'static str, Option<String>); N]) -> Vec<(&'static str, String)> {
    pairs.into_iter().filter_map(|(key, value)| Some((key, value?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jiff::tz::offset;

    fn sony() -> ExifData {
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

    fn new_york() -> TimeZone {
        TimeZone::get("America/New_York").unwrap()
    }

    fn get<'a>(tags: &'a [(&str, String)], key: &str) -> Option<&'a str> {
        tags.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str())
    }

    #[test]
    fn photo_tags_become_video_tags() {
        let exif = sony();
        let tags = build(&exif, exif.gps, None, None, &new_york(), 2030).tags;
        let expected = [
            ("make", "SONY"),
            ("model", "ILCE-7SM3"),
            ("creation_time", "2026-06-01T14:22:10+09:00"),
            ("author", "Kim"),
            ("copyright", "Copyright 2026 Kim"),
            ("location", "+37.566500+126.978000/"),
            ("location-eng", "+37.566500+126.978000/"),
        ];
        assert_eq!(tags, expected.map(|(k, v)| (k, v.to_string())));
    }

    #[test]
    fn apple_keys_keep_local_time_camera_and_lens() {
        let keys = build(&ExifData { offset: None, ..sony() }, None, None, None, &new_york(), 2030).keys;
        let expected = [
            ("com.apple.quicktime.creationdate", "2026-06-01T14:22:10-0400"),
            ("com.apple.quicktime.make", "SONY"),
            ("com.apple.quicktime.model", "ILCE-7SM3"),
            ("com.apple.quicktime.camera.lens_model", "FE 24mm F1.4 GM"),
        ];
        assert_eq!(keys, expected.map(|(k, v)| (k, v.to_string())));
    }

    #[test]
    fn missing_offset_uses_the_computer_zone_at_that_date() {
        let summer = ExifData { offset: None, ..sony() };
        let winter = ExifData { taken_at: Some(date(2026, 1, 15).at(8, 0, 0, 0)), ..summer.clone() };
        let created = |exif: &ExifData| get(&build(exif, None, None, None, &new_york(), 2030).tags, "creation_time").map(str::to_string);
        assert_eq!(created(&summer).as_deref(), Some("2026-06-01T14:22:10-04:00"));
        assert_eq!(created(&winter).as_deref(), Some("2026-01-15T08:00:00-05:00"));
    }

    #[test]
    fn settings_win_over_the_photo_and_blank_settings_do_not() {
        let tags1 = build(&sony(), None, Some("Lee"), Some("Lee Studio"), &new_york(), 2030).tags;
        assert_eq!(get(&tags1, "author"), Some("Lee"));
        assert_eq!(get(&tags1, "copyright"), Some("Copyright 2026 Lee Studio"));
        let tags2 = build(&sony(), None, Some("  "), Some(""), &new_york(), 2030).tags;
        assert_eq!(get(&tags2, "author"), Some("Kim"));
        assert_eq!(get(&tags2, "copyright"), Some("Copyright 2026 Kim"));
    }

    #[test]
    fn copyright_already_written_out_is_kept() {
        for full in ["Copyright 2025 Kim", "copyright Kim", "© 2025 Kim"] {
            let tags = build(&ExifData { copyright: Some(full.into()), ..sony() }, None, None, None, &new_york(), 2030).tags;
            assert_eq!(get(&tags, "copyright"), Some(full));
        }
        let tags = build(&sony(), None, None, Some("Copyright 2024 Lee"), &new_york(), 2030).tags;
        assert_eq!(get(&tags, "copyright"), Some("Copyright 2024 Lee"));
    }

    #[test]
    fn without_capture_time_the_copyright_year_is_this_year() {
        let exif = ExifData { taken_at: None, ..sony() };
        let tags = build(&exif, None, None, None, &new_york(), 2030).tags;
        assert_eq!(get(&tags, "copyright"), Some("Copyright 2030 Kim"));
        assert_eq!(get(&tags, "creation_time"), None);
    }

    #[test]
    fn cleared_location_and_empty_photo_write_nothing() {
        assert_eq!(build(&ExifData::default(), None, None, None, &new_york(), 2030), VideoMetadata::default());
        let edited = build(&sony(), Some(Gps { lat: -33.8568, lon: -74.0445 }), None, None, &new_york(), 2030).tags;
        assert_eq!(get(&edited, "location"), Some("-33.856800-74.044500/"));
    }
}
