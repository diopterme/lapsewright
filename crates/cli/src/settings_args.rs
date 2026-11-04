use clap::{Args, ValueEnum};
use lapsewright_core::settings::{Chroma, Codec, DynamicRange, Fps, MAX_CRF, MotionBlur, Profile, Resolution, Settings};

use crate::store;

/// Video settings. Anything left out comes from `--preset`, or else the default (4K H.264 High SDR).
#[derive(Args)]
pub struct SettingsArgs {
    /// Saved preset to start from; the options below override it
    #[arg(long)]
    preset: Option<String>,
    #[arg(long, value_enum)]
    codec: Option<CodecArg>,
    /// h264: baseline main high high10 high422 high444 | hevc: main main10 main422_10 main444_10 | prores: proxy lt standard hq 4444 4444xq
    #[arg(long)]
    profile: Option<String>,
    #[arg(long, value_enum)]
    chroma: Option<ChromaArg>,
    #[arg(long, value_enum)]
    range: Option<RangeArg>,
    /// WIDTHxHEIGHT, even numbers
    #[arg(long, value_parser = parse_size)]
    size: Option<Resolution>,
    /// 0-51, lower is better (h264, hevc)
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=i64::from(MAX_CRF)))]
    crf: Option<u8>,
    /// N or N/D, e.g. 30000/1001
    #[arg(long)]
    fps: Option<Fps>,
    /// Frames to blend; snapped to 0, 5, 10, 15 or 20
    #[arg(long)]
    blur: Option<u32>,
}

#[derive(Clone, Copy, ValueEnum)]
enum CodecArg {
    H264,
    Hevc,
    Prores,
}

#[derive(Clone, Copy, ValueEnum)]
enum ChromaArg {
    #[value(name = "420")]
    C420,
    #[value(name = "422")]
    C422,
    #[value(name = "444")]
    C444,
}

#[derive(Clone, Copy, ValueEnum)]
enum RangeArg {
    Sdr,
    Hlg,
    Hdr,
}

impl SettingsArgs {
    /// Codec rules are applied later by the builder, so an unsupported chroma or range is corrected there.
    pub fn to_settings(&self) -> Result<Settings, String> {
        let base = match &self.preset {
            Some(id) => store()?.preset(id).map_err(|e| e.to_string())?.settings,
            None => Settings::default(),
        };
        let codec = match self.codec {
            Some(CodecArg::H264) => Codec::H264,
            Some(CodecArg::Hevc) => Codec::Hevc,
            Some(CodecArg::Prores) => Codec::ProRes,
            None => base.profile.codec(),
        };
        let profile = match (&self.profile, self.codec) {
            (Some(id), _) => Profile::find(codec, id).ok_or_else(|| format!("unknown profile `{id}` for {codec:?}"))?,
            (None, Some(_)) => codec.default_profile(),
            (None, None) => base.profile,
        };
        let chroma = match self.chroma {
            Some(ChromaArg::C420) => Chroma::Yuv420,
            Some(ChromaArg::C422) => Chroma::Yuv422,
            Some(ChromaArg::C444) => Chroma::Yuv444,
            None => base.chroma,
        };
        let range = match self.range {
            Some(RangeArg::Sdr) => DynamicRange::Sdr,
            Some(RangeArg::Hlg) => DynamicRange::Hlg,
            Some(RangeArg::Hdr) => DynamicRange::Hdr,
            None => base.range,
        };
        Ok(Settings {
            profile,
            chroma,
            range,
            resolution: self.size.unwrap_or(base.resolution),
            crf: self.crf.unwrap_or(base.crf),
            fps: self.fps.unwrap_or(base.fps),
            blur: self.blur.map(MotionBlur::nearest).unwrap_or(base.blur),
            crop: base.crop,
        })
    }
}

fn parse_size(s: &str) -> Result<Resolution, String> {
    let (w, h) = s.split_once(['x', 'X']).ok_or("expected WIDTHxHEIGHT")?;
    let w = w.parse().map_err(|_| format!("bad width `{w}`"))?;
    let h = h.parse().map_err(|_| format!("bad height `{h}`"))?;
    Resolution::new(w, h).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_takes_either_x() {
        assert_eq!(parse_size("1920x1080"), Resolution::new(1920, 1080).map_err(|e| e.to_string()));
        assert_eq!(parse_size("1920X1080"), Resolution::new(1920, 1080).map_err(|e| e.to_string()));
    }
}
