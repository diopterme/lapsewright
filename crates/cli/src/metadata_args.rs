use std::path::Path;

use clap::Args;
use jiff::Zoned;
use lapsewright_core::exif::{self, Gps};
use lapsewright_core::metadata::{self, VideoMetadata};

/// What goes into the video besides the first photo's EXIF.
#[derive(Args)]
pub struct MetadataArgs {
    /// Author; default: the first photo's Artist
    #[arg(long)]
    author: Option<String>,
    /// Copyright holder, written as "Copyright <year> <holder>"; default: the first photo's Copyright
    #[arg(long)]
    copyright: Option<String>,
    /// LAT,LON in decimal degrees, replacing the photo's location
    #[arg(long, value_parser = parse_location, allow_hyphen_values = true, conflicts_with = "no_location")]
    location: Option<Gps>,
    /// Leave the location out
    #[arg(long)]
    no_location: bool,
}

impl MetadataArgs {
    pub fn build(&self, first_photo: &Path) -> Result<VideoMetadata, String> {
        let exif = exif::read(first_photo).map_err(|e| format!("{}: {e}", first_photo.display()))?;
        let gps = if self.no_location { None } else { self.location.or(exif.gps) };
        let now = Zoned::now();
        Ok(metadata::build(&exif, gps, self.author.as_deref(), self.copyright.as_deref(), now.time_zone(), now.year()))
    }
}

fn parse_location(s: &str) -> Result<Gps, String> {
    let (lat, lon) = s.split_once(',').ok_or("expected LAT,LON")?;
    let degrees = |text: &str, limit: f64| {
        let value: f64 = text.trim().parse().map_err(|_| format!("bad number `{text}`"))?;
        if value.abs() <= limit { Ok(value) } else { Err(format!("`{text}` is outside ±{limit}")) }
    };
    Ok(Gps { lat: degrees(lat, 90.0)?, lon: degrees(lon, 180.0)? })
}
