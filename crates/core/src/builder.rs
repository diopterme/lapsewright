use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::metadata::VideoMetadata;
use crate::rules;
use crate::settings::{CROP_END, Codec, DynamicRange, MotionBlur, Settings};

/// Display P3 primaries, D65, 1000 / 0.005 nits, in the units `-mastering_display` expects.
const MASTERING_DISPLAY: &str = "G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50)";
/// MaxCLL, MaxFALL in nits.
const CONTENT_LIGHT: &str = "1000,400";
const SWS_FLAGS: &str = "accurate_rnd+bitexact+full_chroma_int";
/// Restores edge contrast lost to downscaling and blending.
const UNSHARP: &str = "unsharp=luma_msize_x=7:luma_msize_y=7:luma_amount=0.4";
/// Measured moov: about 4 KiB plus 12 bytes per frame (H.264, HEVC) or 4 (ProRes).
/// Generous, since ffmpeg fails the whole run when the room is too small.
const MOOV_BASE: usize = 64 * 1024;
const MOOV_PER_FRAME: usize = 32;

/// Reads `-i` as numbered photos from 1 (`convert::Sequence`).
pub(crate) const SEQUENCE_INPUT: [&str; 7] = ["-f", "image2", "-pattern_type", "sequence", "-start_number", "1", "-i"];

/// Full ffmpeg argument list (without the program name) for one conversion of `frames` photos.
/// `photos`: an image2 pattern such as `/tmp/x/%06d.jxl`.
pub fn build(settings: &Settings, photos: &Path, frames: usize, metadata: &VideoMetadata, output: &Path) -> Vec<OsString> {
    let s = rules::normalize(*settings);
    let fps = s.fps.to_string();
    let color = Color::of(s.range);
    // `-y`: the output is an empty file `output::reserve` created for this run.
    // `-xerror`: without it a broken photo silently ends the video there and ffmpeg still exits 0.
    let input = [
        owned(&["-hide_banner", "-nostats", "-xerror", "-y", "-progress", "pipe:1"]),
        hdr_input_options(s.range),
        owned(&["-r", &fps]),
        owned(&SEQUENCE_INPUT),
    ]
    .concat();
    let encode = [
        vec!["-filter_complex".to_string(), filter_chain(&s, &color)],
        codec_options(&s, &color),
        owned(&[
            "-sws_flags", SWS_FLAGS,
            // `-moov_size` instead of faststart: moov still goes first, followed by free room
            // that `quicktime::add_keys` fills without moving the video data.
            "-movflags", "write_colr", "-moov_size", &moov_reserve(frames, metadata).to_string(), "-an",
            "-color_range", "tv",
            "-color_primaries", color.primaries,
            "-colorspace", color.matrix,
            "-color_trc", color.transfer,
            "-r", &fps,
        ]),
        metadata.tags.iter().flat_map(|(key, value)| ["-metadata".to_string(), format!("{key}={value}")]).collect(),
    ]
    .concat();
    input
        .into_iter()
        .map(OsString::from)
        .chain([photos.as_os_str().to_owned()])
        .chain(encode.into_iter().map(OsString::from))
        .chain([output.as_os_str().to_owned()])
        .collect()
}

/// `ffmpeg` and `args` as one shell line, for showing (`lapsewright args`, the app's log).
/// The real call passes arguments without a shell.
pub fn command_line(args: &[OsString]) -> String {
    std::iter::once(OsStr::new("ffmpeg")).chain(args.iter().map(OsString::as_os_str)).map(shell_quote).collect::<Vec<_>>().join(" ")
}

fn shell_quote(arg: &OsStr) -> String {
    let s = arg.to_string_lossy();
    let is_plain = !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-./:=+,@%".contains(c));
    if is_plain { s.into_owned() } else { format!("'{}'", s.replace('\'', r"'\''")) }
}

/// Tags only: pixels are not converted, the photos must already be graded for the chosen range.
struct Color {
    zscale_matrix: &'static str,
    zscale_primaries: &'static str,
    transfer: &'static str,
    matrix: &'static str,
    primaries: &'static str,
}

impl Color {
    fn of(range: DynamicRange) -> Color {
        let (zscale_matrix, zscale_primaries, transfer, matrix, primaries) = match range {
            DynamicRange::Sdr => ("bt709", "bt709", "bt709", "bt709", "bt709"),
            DynamicRange::Hlg => ("2020_ncl", "2020", "arib-std-b67", "bt2020nc", "bt2020"),
            DynamicRange::Hdr => ("2020_ncl", "2020", "smpte2084", "bt2020nc", "bt2020"),
        };
        Color { zscale_matrix, zscale_primaries, transfer, matrix, primaries }
    }
}

fn moov_reserve(frames: usize, metadata: &VideoMetadata) -> usize {
    let text: usize = metadata.tags.iter().chain(&metadata.keys).map(|(key, value)| key.len() + value.len()).sum();
    MOOV_BASE + MOOV_PER_FRAME * frames + 2 * text
}

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Input-side options: ffmpeg attaches them to the decoded frames, and every encoder
/// and the mov muxer carry them on (verified with x264, x265 and prores_ks).
fn hdr_input_options(range: DynamicRange) -> Vec<String> {
    match range {
        DynamicRange::Hdr => owned(&["-mastering_display", MASTERING_DISPLAY, "-content_light", CONTENT_LIGHT]),
        DynamicRange::Sdr | DynamicRange::Hlg => Vec::new(),
    }
}

fn filter_chain(s: &Settings, color: &Color) -> String {
    let (w, h) = (s.resolution.width(), s.resolution.height());
    let g = gcd(w, h);
    let (rw, rh) = (w / g, h / g);
    // Crop to the target aspect first so zscale never stretches. Only one side has room to
    // spare, so the same position works for x and y.
    let at = f64::from(s.crop.min(CROP_END)) / f64::from(CROP_END);
    let crop = format!("crop=w='floor(min(iw,ih*{rw}/{rh})/2)*2':h='floor(min(ih,iw*{rh}/{rw})/2)*2':x='(iw-ow)*{at:.3}':y='(ih-oh)*{at:.3}'");
    let (m, p, t) = (color.zscale_matrix, color.zscale_primaries, color.transfer);
    let zscale = format!(
        "zscale=rangein=full:range=limited:matrixin={m}:matrix={m}:primariesin={p}:primaries={p}:transferin={t}:transfer={t}:w={w}:h={h}"
    );
    [Some(crop), Some(zscale), Some("setsar=sar=1/1".to_string()), blend_chain(s.blur), Some(UNSHARP.to_string())]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn blend_chain(blur: MotionBlur) -> Option<String> {
    let opacity = match blur {
        MotionBlur::Off => return None,
        MotionBlur::Frames5 => "0.45",
        MotionBlur::Frames10 => "0.63",
        MotionBlur::Frames15 => "0.75",
        MotionBlur::Frames20 => "0.82",
    };
    let n = blur.frames();
    let split = format!("split={n}{}", (0..n).map(|i| format!("[s{i}]")).collect::<String>());
    let shifts = (0..n).map(|i| format!("[s{i}]setpts=PTS-{i}[t{i}]"));
    // The last blend stays unlabeled so it feeds the next filter of the chain.
    let blends = (1..n).map(|i| {
        let from = if i == 1 { "[t0]".to_string() } else { format!("[m{}]", i - 2) };
        let to = if i == n - 1 { String::new() } else { format!("[m{}]", i - 1) };
        format!("{from}[t{i}]blend=all_mode=normal:all_opacity={opacity}{to}")
    });
    Some(std::iter::once(split).chain(shifts).chain(blends).collect::<Vec<_>>().join(";"))
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn codec_options(s: &Settings, color: &Color) -> Vec<String> {
    let spec = s.profile.spec();
    let encoder = match spec.codec {
        Codec::H264 => "libx264",
        Codec::Hevc => "libx265",
        Codec::ProRes => "prores_ks",
    };
    let crf = s.crf.to_string();
    let common = owned(&["-c:v", encoder, "-pix_fmt", rules::pix_fmt(s), "-profile:v", spec.ffmpeg]);
    let specific = match spec.codec {
        Codec::H264 => owned(&["-crf", &crf, "-preset", "slow", "-tune", "stillimage"]),
        // hvc1 instead of hev1: QuickTime and Finder only play hvc1.
        Codec::Hevc => [owned(&["-crf", &crf, "-preset", "slow", "-tag:v", "hvc1"]), x265_params(s.range, color)].concat(),
        Codec::ProRes => {
            let bsf = format!(
                "prores_metadata=color_primaries={}:color_trc={}:colorspace={}",
                color.primaries, color.transfer, color.matrix
            );
            owned(&["-vendor", "apl0", "-bsf:v", &bsf])
        }
    };
    [common, specific].concat()
}

fn x265_params(range: DynamicRange, color: &Color) -> Vec<String> {
    let base = format!("colorprim={}:transfer={}:colormatrix={}", color.primaries, color.transfer, color.matrix);
    match range {
        DynamicRange::Sdr => Vec::new(),
        DynamicRange::Hlg => vec!["-x265-params".to_string(), base],
        DynamicRange::Hdr => vec!["-x265-params".to_string(), format!("{base}:hdr-opt=1:repeat-headers=1")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Chroma, Profile, Resolution};

    fn strs(args: &[OsString]) -> Vec<&str> {
        args.iter().map(|a| a.to_str().unwrap()).collect()
    }

    fn run(settings: Settings, output: &str) -> Vec<OsString> {
        build(&settings, Path::new("/tmp/x/%06d.jpg"), 24, &VideoMetadata::default(), Path::new(output))
    }

    const SDR_ZSCALE: &str = "zscale=rangein=full:range=limited:matrixin=bt709:matrix=bt709:primariesin=bt709:primaries=bt709:transferin=bt709:transfer=bt709";
    const BLUR5: &str = "split=5[s0][s1][s2][s3][s4];[s0]setpts=PTS-0[t0];[s1]setpts=PTS-1[t1];[s2]setpts=PTS-2[t2];[s3]setpts=PTS-3[t3];[s4]setpts=PTS-4[t4];[t0][t1]blend=all_mode=normal:all_opacity=0.45[m0];[m0][t2]blend=all_mode=normal:all_opacity=0.45[m1];[m1][t3]blend=all_mode=normal:all_opacity=0.45[m2];[m2][t4]blend=all_mode=normal:all_opacity=0.45";
    const CROP_16_9: &str = "crop=w='floor(min(iw,ih*16/9)/2)*2':h='floor(min(ih,iw*9/16)/2)*2':x='(iw-ow)*0.500':y='(ih-oh)*0.500'";

    #[test]
    fn command_line_quotes_only_what_the_shell_would_split() {
        let args = ["-i", "/a b/it's.txt", "-metadata", "make=Sony", ""].map(OsString::from);
        assert_eq!(command_line(&args), r"ffmpeg -i '/a b/it'\''s.txt' -metadata make=Sony ''");
    }

    #[test]
    fn golden_h264_high_sdr_blur5() {
        let chain = format!("{CROP_16_9},{SDR_ZSCALE}:w=4096:h=2304,setsar=sar=1/1,{BLUR5},{UNSHARP}");
        let expected = [
            "-hide_banner", "-nostats", "-xerror", "-y", "-progress", "pipe:1",
            "-r", "30000/1001", "-f", "image2", "-pattern_type", "sequence", "-start_number", "1", "-i", "/tmp/x/%06d.jpg",
            "-filter_complex", &chain,
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-profile:v", "high",
            "-crf", "8", "-preset", "slow", "-tune", "stillimage",
            "-sws_flags", SWS_FLAGS,
            "-movflags", "write_colr", "-moov_size", "66304", "-an",
            "-color_range", "tv", "-color_primaries", "bt709", "-colorspace", "bt709", "-color_trc", "bt709",
            "-r", "30000/1001",
            "/out/day.mp4",
        ];
        assert_eq!(strs(&run(Settings::default(), "/out/day.mp4")), expected);
    }

    #[test]
    fn crop_follows_the_position_and_stays_inside() {
        let chain = |crop| filter_chain(&Settings { crop, ..Settings::default() }, &Color::of(DynamicRange::Sdr));
        assert!(chain(0).starts_with("crop=w='floor(min(iw,ih*16/9)/2)*2':h='floor(min(ih,iw*9/16)/2)*2':x='(iw-ow)*0.000':y='(ih-oh)*0.000',"));
        assert!(chain(250).contains(":x='(iw-ow)*0.250':y='(ih-oh)*0.250',"));
        assert!(chain(u16::MAX).contains(":x='(iw-ow)*1.000':y='(ih-oh)*1.000',"));
    }

    #[test]
    fn golden_prores_hq_hdr_blur5() {
        let settings = Settings {
            profile: Profile::ProResHq,
            chroma: Chroma::Yuv422,
            range: DynamicRange::Hdr,
            resolution: Resolution::new(3840, 2160).unwrap(),
            ..Settings::default()
        };
        let chain = format!(
            "{CROP_16_9},zscale=rangein=full:range=limited:matrixin=2020_ncl:matrix=2020_ncl:primariesin=2020:primaries=2020:transferin=smpte2084:transfer=smpte2084:w=3840:h=2160,setsar=sar=1/1,{BLUR5},{UNSHARP}"
        );
        let expected = [
            "-hide_banner", "-nostats", "-xerror", "-y", "-progress", "pipe:1",
            "-mastering_display", MASTERING_DISPLAY, "-content_light", CONTENT_LIGHT,
            "-r", "30000/1001", "-f", "image2", "-pattern_type", "sequence", "-start_number", "1", "-i", "/tmp/x/%06d.jpg",
            "-filter_complex", &chain,
            "-c:v", "prores_ks", "-pix_fmt", "yuv422p10le", "-profile:v", "hq",
            "-vendor", "apl0",
            "-bsf:v", "prores_metadata=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc",
            "-sws_flags", SWS_FLAGS,
            "-movflags", "write_colr", "-moov_size", "66304", "-an",
            "-color_range", "tv", "-color_primaries", "bt2020", "-colorspace", "bt2020nc", "-color_trc", "smpte2084",
            "-r", "30000/1001",
            "/out/day.mov",
        ];
        assert_eq!(strs(&run(settings, "/out/day.mov")), expected);
    }

    #[test]
    fn golden_hevc_main10_hlg_no_blur() {
        let settings = Settings {
            profile: Profile::HevcMain10,
            range: DynamicRange::Hlg,
            resolution: Resolution::new(4096, 2160).unwrap(),
            crf: 12,
            fps: "25".parse().unwrap(),
            blur: MotionBlur::Off,
            ..Settings::default()
        };
        let chain = format!(
            "crop=w='floor(min(iw,ih*256/135)/2)*2':h='floor(min(ih,iw*135/256)/2)*2':x='(iw-ow)*0.500':y='(ih-oh)*0.500',zscale=rangein=full:range=limited:matrixin=2020_ncl:matrix=2020_ncl:primariesin=2020:primaries=2020:transferin=arib-std-b67:transfer=arib-std-b67:w=4096:h=2160,setsar=sar=1/1,{UNSHARP}"
        );
        let expected = [
            "-hide_banner", "-nostats", "-xerror", "-y", "-progress", "pipe:1",
            "-r", "25", "-f", "image2", "-pattern_type", "sequence", "-start_number", "1", "-i", "/tmp/x/%06d.jpg",
            "-filter_complex", &chain,
            "-c:v", "libx265", "-pix_fmt", "yuv420p10le", "-profile:v", "main10",
            "-crf", "12", "-preset", "slow", "-tag:v", "hvc1",
            "-x265-params", "colorprim=bt2020:transfer=arib-std-b67:colormatrix=bt2020nc",
            "-sws_flags", SWS_FLAGS,
            "-movflags", "write_colr", "-moov_size", "66304", "-an",
            "-color_range", "tv", "-color_primaries", "bt2020", "-colorspace", "bt2020nc", "-color_trc", "arib-std-b67",
            "-r", "25",
            "/out/day.mov",
        ];
        assert_eq!(strs(&run(settings, "/out/day.mov")), expected);
    }

    #[test]
    fn hevc_hdr_adds_hdr10_x265_params() {
        let settings = Settings { profile: Profile::HevcMain10, range: DynamicRange::Hdr, ..Settings::default() };
        let args = run(settings, "/out/day.mov");
        let args = strs(&args);
        let i = args.iter().position(|a| *a == "-x265-params").unwrap();
        assert_eq!(
            args[i + 1],
            "colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:hdr-opt=1:repeat-headers=1"
        );
        assert!(args.contains(&"-mastering_display"));
    }

    #[test]
    fn blur20_chains_nineteen_blends_with_last_unlabeled() {
        let settings = Settings { blur: MotionBlur::Frames20, ..Settings::default() };
        let args = run(settings, "/out/day.mp4");
        let args = strs(&args);
        let chain = args[args.iter().position(|a| *a == "-filter_complex").unwrap() + 1];
        assert!(chain.contains("split=20[s0]"));
        assert_eq!(chain.matches("blend=").count(), 19);
        assert!(chain.ends_with(&format!("[m17][t19]blend=all_mode=normal:all_opacity=0.82,{UNSHARP}")));
    }

    #[test]
    fn metadata_goes_right_before_the_output() {
        let metadata = VideoMetadata {
            tags: vec![("make", "SONY".to_string()), ("copyright", "Copyright 2026 Kim".to_string())],
            keys: vec![("com.apple.quicktime.camera.lens_model", "FE 24mm F1.4 GM".to_string())],
        };
        let args = build(&Settings::default(), Path::new("/tmp/x/%06d.jpg"), 24, &metadata, Path::new("/out/day.mp4"));
        let args = strs(&args);
        let tail = ["-metadata", "make=SONY", "-metadata", "copyright=Copyright 2026 Kim", "/out/day.mp4"];
        assert_eq!(args[args.len() - tail.len()..], tail);
    }

    #[test]
    fn moov_room_grows_with_frames_and_metadata() {
        let metadata = VideoMetadata { tags: vec![("make", "SONY".to_string())], keys: Vec::new() };
        assert_eq!(moov_reserve(0, &VideoMetadata::default()), MOOV_BASE);
        assert_eq!(moov_reserve(1000, &metadata), MOOV_BASE + 32 * 1000 + 2 * 8);
    }

    #[test]
    fn invalid_combination_is_normalized_before_building() {
        let settings = Settings { profile: Profile::H264High10, chroma: Chroma::Yuv422, ..Settings::default() };
        let args = run(settings, "/out/day.mp4");
        assert!(strs(&args).contains(&"yuv420p10le"));
    }
}
