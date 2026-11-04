use crate::settings::{Chroma, Codec, DynamicRange, Profile, Settings};

pub struct ProfileSpec {
    pub codec: Codec,
    /// Our name for the profile (CLI, presets).
    pub id: &'static str,
    /// Value for `-profile:v`.
    pub ffmpeg: &'static str,
    pub bits: u8,
    /// First entry is the fallback when the current chroma is not allowed.
    pub chroma: &'static [Chroma],
    pub hdr: bool,
}

const C420: &[Chroma] = &[Chroma::Yuv420];
const C422: &[Chroma] = &[Chroma::Yuv422];
const C444: &[Chroma] = &[Chroma::Yuv444];
const UP_TO_422: &[Chroma] = &[Chroma::Yuv420, Chroma::Yuv422];
const UP_TO_444: &[Chroma] = &[Chroma::Yuv420, Chroma::Yuv422, Chroma::Yuv444];

impl Profile {
    pub const ALL: [Profile; 16] = [
        Profile::H264Baseline,
        Profile::H264Main,
        Profile::H264High,
        Profile::H264High10,
        Profile::H264High422,
        Profile::H264High444,
        Profile::HevcMain,
        Profile::HevcMain10,
        Profile::HevcMain422_10,
        Profile::HevcMain444_10,
        Profile::ProResProxy,
        Profile::ProResLt,
        Profile::ProResStandard,
        Profile::ProResHq,
        Profile::ProRes4444,
        Profile::ProRes4444Xq,
    ];

    pub fn spec(self) -> ProfileSpec {
        use Codec::*;
        let (codec, id, ffmpeg, bits, chroma, hdr) = match self {
            // x264 rejects 4:2:2 under high10.
            Profile::H264Baseline => (H264, "baseline", "baseline", 8, C420, false),
            Profile::H264Main => (H264, "main", "main", 8, C420, false),
            Profile::H264High => (H264, "high", "high", 8, C420, false),
            Profile::H264High10 => (H264, "high10", "high10", 10, C420, true),
            Profile::H264High422 => (H264, "high422", "high422", 10, UP_TO_422, true),
            Profile::H264High444 => (H264, "high444", "high444", 10, UP_TO_444, true),
            Profile::HevcMain => (Hevc, "main", "main", 8, C420, false),
            Profile::HevcMain10 => (Hevc, "main10", "main10", 10, C420, true),
            Profile::HevcMain422_10 => (Hevc, "main422_10", "main422-10", 10, UP_TO_422, true),
            Profile::HevcMain444_10 => (Hevc, "main444_10", "main444-10", 10, UP_TO_444, true),
            // prores_ks takes 10-bit input for every profile, 4444 XQ included.
            Profile::ProResProxy => (ProRes, "proxy", "proxy", 10, C422, false),
            Profile::ProResLt => (ProRes, "lt", "lt", 10, C422, false),
            Profile::ProResStandard => (ProRes, "standard", "standard", 10, C422, true),
            Profile::ProResHq => (ProRes, "hq", "hq", 10, C422, true),
            Profile::ProRes4444 => (ProRes, "4444", "4444", 10, C444, true),
            Profile::ProRes4444Xq => (ProRes, "4444xq", "4444xq", 10, C444, true),
        };
        ProfileSpec { codec, id, ffmpeg, bits, chroma, hdr }
    }

    pub fn codec(self) -> Codec {
        self.spec().codec
    }

    pub fn find(codec: Codec, id: &str) -> Option<Profile> {
        Profile::ALL.into_iter().find(|p| {
            let spec = p.spec();
            spec.codec == codec && spec.id == id
        })
    }
}

/// Replaces chroma, dynamic range and crf the profile cannot encode with allowed ones.
pub fn normalize(settings: Settings) -> Settings {
    let spec = settings.profile.spec();
    let chroma = if spec.chroma.contains(&settings.chroma) { settings.chroma } else { spec.chroma[0] };
    let range = if spec.hdr { settings.range } else { DynamicRange::Sdr };
    // x264 crf 0 is lossless, which the 8-bit profiles reject.
    let is_8bit_h264 = spec.codec == Codec::H264 && spec.bits == 8;
    let crf = if is_8bit_h264 { settings.crf.max(1) } else { settings.crf };
    Settings { chroma, range, crf, ..settings }
}

/// Expects normalized settings.
pub fn pix_fmt(settings: &Settings) -> &'static str {
    let is_10bit = settings.profile.spec().bits == 10;
    match (settings.chroma, is_10bit) {
        (Chroma::Yuv420, false) => "yuv420p",
        (Chroma::Yuv420, true) => "yuv420p10le",
        (Chroma::Yuv422, false) => "yuv422p",
        (Chroma::Yuv422, true) => "yuv422p10le",
        (Chroma::Yuv444, false) => "yuv444p",
        (Chroma::Yuv444, true) => "yuv444p10le",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(profile: Profile, chroma: Chroma, range: DynamicRange) -> Settings {
        Settings { profile, chroma, range, ..Settings::default() }
    }

    #[test]
    fn finds_profile_by_codec_and_id() {
        assert_eq!(Profile::find(Codec::Hevc, "main"), Some(Profile::HevcMain));
        assert_eq!(Profile::find(Codec::H264, "main"), Some(Profile::H264Main));
        assert_eq!(Profile::find(Codec::ProRes, "main"), None);
    }

    #[test]
    fn profile_ids_are_unique_per_codec() {
        for p in Profile::ALL {
            assert_eq!(Profile::find(p.codec(), p.spec().id), Some(p));
        }
    }

    #[test]
    fn high10_forces_420() {
        let s = normalize(with(Profile::H264High10, Chroma::Yuv422, DynamicRange::Hdr));
        assert_eq!(s.chroma, Chroma::Yuv420);
        assert_eq!(s.range, DynamicRange::Hdr);
    }

    #[test]
    fn eight_bit_profile_drops_hdr() {
        let s = normalize(with(Profile::HevcMain, Chroma::Yuv420, DynamicRange::Hlg));
        assert_eq!(s.range, DynamicRange::Sdr);
    }

    #[test]
    fn prores_chroma_is_fixed() {
        assert_eq!(normalize(with(Profile::ProResHq, Chroma::Yuv420, DynamicRange::Sdr)).chroma, Chroma::Yuv422);
        assert_eq!(normalize(with(Profile::ProRes4444Xq, Chroma::Yuv420, DynamicRange::Sdr)).chroma, Chroma::Yuv444);
    }

    #[test]
    fn eight_bit_h264_raises_lossless_crf() {
        for profile in [Profile::H264Baseline, Profile::H264Main, Profile::H264High] {
            let s = Settings { crf: 0, ..with(profile, Chroma::Yuv420, DynamicRange::Sdr) };
            assert_eq!(normalize(s).crf, 1, "{profile:?}");
        }
        for profile in [Profile::H264High10, Profile::HevcMain] {
            let s = Settings { crf: 0, ..with(profile, Chroma::Yuv420, DynamicRange::Sdr) };
            assert_eq!(normalize(s).crf, 0, "{profile:?}");
        }
    }

    #[test]
    fn allowed_values_are_kept() {
        let s = with(Profile::HevcMain444_10, Chroma::Yuv422, DynamicRange::Hlg);
        assert_eq!(normalize(s), s);
    }

    #[test]
    fn pix_fmt_follows_chroma_and_bits() {
        let cases = [
            (Profile::H264High, Chroma::Yuv420, "yuv420p"),
            (Profile::H264High444, Chroma::Yuv422, "yuv422p10le"),
            (Profile::HevcMain444_10, Chroma::Yuv444, "yuv444p10le"),
            (Profile::ProResHq, Chroma::Yuv422, "yuv422p10le"),
            (Profile::ProRes4444Xq, Chroma::Yuv444, "yuv444p10le"),
        ];
        for (profile, chroma, expected) in cases {
            assert_eq!(pix_fmt(&with(profile, chroma, DynamicRange::Sdr)), expected, "{profile:?}");
        }
    }
}
