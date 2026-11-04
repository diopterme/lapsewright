use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    Hevc,
    ProRes,
}

impl Codec {
    pub fn default_profile(self) -> Profile {
        match self {
            Codec::H264 => Profile::H264High,
            Codec::Hevc => Profile::HevcMain10,
            Codec::ProRes => Profile::ProResHq,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Codec::H264 => "mp4",
            Codec::Hevc | Codec::ProRes => "mov",
        }
    }
}

/// The profile implies the codec, so a codec/profile mismatch cannot be represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    H264Baseline,
    H264Main,
    H264High,
    H264High10,
    H264High422,
    H264High444,
    HevcMain,
    HevcMain10,
    HevcMain422_10,
    HevcMain444_10,
    ProResProxy,
    ProResLt,
    ProResStandard,
    ProResHq,
    ProRes4444,
    ProRes4444Xq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chroma {
    Yuv420,
    Yuv422,
    Yuv444,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DynamicRange {
    Sdr,
    Hlg,
    /// HDR10 (PQ)
    Hdr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    width: u32,
    height: u32,
}

/// Longest side any profile accepts; past it ffmpeg fails mid-run.
pub const MAX_SIDE: u32 = 8192;

impl Resolution {
    /// Even sizes only: 4:2:0 and 4:2:2 encoders reject odd dimensions.
    pub fn new(width: u32, height: u32) -> Result<Self, SettingsError> {
        let is_valid = |n: u32| n > 0 && n <= MAX_SIDE && n % 2 == 0;
        if is_valid(width) && is_valid(height) {
            Ok(Resolution { width, height })
        } else {
            Err(SettingsError::Resolution(width, height))
        }
    }

    pub fn width(self) -> u32 {
        self.width
    }

    pub fn height(self) -> u32 {
        self.height
    }
}

/// Kept as a fraction so NTSC rates like 30000/1001 stay exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fps {
    num: u32,
    den: u32,
}

impl FromStr for Fps {
    type Err = SettingsError;

    /// Accepts `N` or `N/D` with positive integers.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (num, den) = s.split_once('/').unwrap_or((s, "1"));
        let positive = |part: &str| part.parse::<u32>().ok().filter(|n| *n > 0);
        match (positive(num), positive(den)) {
            (Some(num), Some(den)) => Ok(Fps { num, den }),
            _ => Err(SettingsError::Fps(s.to_string())),
        }
    }
}

impl fmt::Display for Fps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

/// Only these frame counts have a tuned blend opacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionBlur {
    Off,
    Frames5,
    Frames10,
    Frames15,
    Frames20,
}

impl MotionBlur {
    pub fn nearest(frames: u32) -> Self {
        match frames {
            0..=2 => MotionBlur::Off,
            3..=7 => MotionBlur::Frames5,
            8..=12 => MotionBlur::Frames10,
            13..=17 => MotionBlur::Frames15,
            _ => MotionBlur::Frames20,
        }
    }

    pub fn frames(self) -> u32 {
        match self {
            MotionBlur::Off => 0,
            MotionBlur::Frames5 => 5,
            MotionBlur::Frames10 => 10,
            MotionBlur::Frames15 => 15,
            MotionBlur::Frames20 => 20,
        }
    }
}

pub const MAX_CRF: u8 = 51;
/// `Settings::crop` runs from 0 (top or left edge) to this (bottom or right edge).
pub const CROP_END: u16 = 1000;
pub const CROP_CENTER: u16 = CROP_END / 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub profile: Profile,
    pub chroma: Chroma,
    pub range: DynamicRange,
    pub resolution: Resolution,
    /// 0–`MAX_CRF`, lower is better. Ignored by ProRes.
    pub crf: u8,
    pub fps: Fps,
    pub blur: MotionBlur,
    /// Where the video's frame sits along the side the photo has to spare, 0–`CROP_END`.
    /// Per folder, not in presets (`SettingsFile` leaves it out).
    pub crop: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            profile: Profile::H264High,
            chroma: Chroma::Yuv420,
            range: DynamicRange::Sdr,
            resolution: Resolution { width: 4096, height: 2304 },
            crf: 8,
            fps: Fps { num: 30000, den: 1001 },
            blur: MotionBlur::Frames5,
            crop: CROP_CENTER,
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("resolution must be even numbers from 2 to {MAX_SIDE}: {0}x{1}")]
    Resolution(u32, u32),
    #[error("frame rate must be N or N/D with positive integers: {0}")]
    Fps(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_rejects_zero_and_odd() {
        assert!(Resolution::new(1920, 1080).is_ok());
        assert_eq!(Resolution::new(0, 1080), Err(SettingsError::Resolution(0, 1080)));
        assert_eq!(Resolution::new(1921, 1080), Err(SettingsError::Resolution(1921, 1080)));
        assert_eq!(Resolution::new(1920, 1079), Err(SettingsError::Resolution(1920, 1079)));
    }

    #[test]
    fn resolution_has_upper_bound() {
        assert!(Resolution::new(MAX_SIDE, MAX_SIDE).is_ok());
        assert_eq!(Resolution::new(MAX_SIDE + 2, 1080), Err(SettingsError::Resolution(MAX_SIDE + 2, 1080)));
        assert_eq!(Resolution::new(1920, 100_000), Err(SettingsError::Resolution(1920, 100_000)));
    }

    #[test]
    fn fps_parses_fraction_and_integer() {
        assert_eq!("30000/1001".parse(), Ok(Fps { num: 30000, den: 1001 }));
        assert_eq!("25".parse(), Ok(Fps { num: 25, den: 1 }));
    }

    #[test]
    fn fps_rejects_malformed() {
        for bad in ["", "29.97", "0", "30/0", "/1001", "30000/", "a/b", "-30", "30/1/1"] {
            assert_eq!(bad.parse::<Fps>(), Err(SettingsError::Fps(bad.to_string())), "{bad}");
        }
    }

    #[test]
    fn fps_displays_for_ffmpeg() {
        assert_eq!(Fps { num: 30000, den: 1001 }.to_string(), "30000/1001");
        assert_eq!(Fps { num: 24, den: 1 }.to_string(), "24");
    }

    #[test]
    fn blur_snaps_to_nearest_level() {
        let cases = [(0, 0), (2, 0), (3, 5), (7, 5), (8, 10), (12, 10), (13, 15), (18, 20), (99, 20)];
        for (input, frames) in cases {
            assert_eq!(MotionBlur::nearest(input).frames(), frames, "{input}");
        }
    }
}
