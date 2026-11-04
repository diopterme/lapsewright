use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

/// zscale needs ffmpeg built with zimg, so it is not guaranteed.
pub const REQUIRED_FILTERS: &[&str] = &["zscale"];
pub const REQUIRED_ENCODERS: &[&str] = &["libx264", "libx265", "prores_ks"];
/// JXL photos only decode through libjxl.
pub const REQUIRED_DECODERS: &[&str] = &["libjxl"];
/// HDR (PQ) light levels for every codec; added in ffmpeg 9, so 8.x fails each PQ run.
pub const REQUIRED_OPTIONS: &[&str] = &["mastering_display", "content_light"];

#[derive(Debug, thiserror::Error)]
pub enum BinaryError {
    #[error("ffmpeg not found: {0}")]
    NotFound(PathBuf),
    #[error("failed to run {path}: {source}")]
    Spawn { path: PathBuf, source: io::Error },
    #[error("{path} exited with {status}")]
    Failed { path: PathBuf, status: ExitStatus },
    #[error("unrecognized `ffmpeg -version` output")]
    UnknownVersion,
}

#[derive(Debug)]
pub struct CheckReport {
    pub path: PathBuf,
    pub version: String,
    pub missing_filters: Vec<&'static str>,
    pub missing_encoders: Vec<&'static str>,
    pub missing_decoders: Vec<&'static str>,
    pub missing_options: Vec<&'static str>,
}

impl CheckReport {
    pub fn is_ok(&self) -> bool {
        self.missing_filters.is_empty()
            && self.missing_encoders.is_empty()
            && self.missing_decoders.is_empty()
            && self.missing_options.is_empty()
    }
}

/// Without an override, ffmpeg is expected next to our own executable (bundled).
pub fn locate(override_path: Option<&Path>) -> Result<PathBuf, BinaryError> {
    let path = match override_path {
        Some(p) => p.to_path_buf(),
        None => bundled_path(),
    };
    if path.is_file() {
        Ok(path)
    } else {
        Err(BinaryError::NotFound(path))
    }
}

fn bundled_path() -> PathBuf {
    let name = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

pub fn check(ffmpeg: &Path) -> Result<CheckReport, BinaryError> {
    let version = run(ffmpeg, &["-version"])?;
    let version = parse_version(&version).ok_or(BinaryError::UnknownVersion)?;
    let filters = run(ffmpeg, &["-hide_banner", "-filters"])?;
    let encoders = run(ffmpeg, &["-hide_banner", "-encoders"])?;
    let decoders = run(ffmpeg, &["-hide_banner", "-decoders"])?;
    let help = run(ffmpeg, &["-hide_banner", "-h", "full"])?;
    Ok(CheckReport {
        path: ffmpeg.to_path_buf(),
        version: version.to_string(),
        missing_filters: missing(REQUIRED_FILTERS, &list_names(&filters)),
        missing_encoders: missing(REQUIRED_ENCODERS, &list_names(&encoders)),
        missing_decoders: missing(REQUIRED_DECODERS, &list_names(&decoders)),
        missing_options: missing(REQUIRED_OPTIONS, &list_options(&help)),
    })
}

fn run(ffmpeg: &Path, args: &[&str]) -> Result<String, BinaryError> {
    let out = Command::new(ffmpeg)
        .args(args)
        .output()
        .map_err(|source| BinaryError::Spawn { path: ffmpeg.to_path_buf(), source })?;
    if !out.status.success() {
        return Err(BinaryError::Failed { path: ffmpeg.to_path_buf(), status: out.status });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_version(out: &str) -> Option<&str> {
    out.lines().next()?.strip_prefix("ffmpeg version ")?.split_whitespace().next()
}

/// Rows of `-filters` / `-encoders` / `-decoders` follow a `------` line; the name is the second column.
fn list_names(out: &str) -> Vec<&str> {
    out.lines()
        .skip_while(|line| line.trim() != "------")
        .skip(1)
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect()
}

/// `-name[:<stream_spec>]  description` rows of `-h full`.
fn list_options(help: &str) -> Vec<&str> {
    help.lines()
        .filter_map(|line| line.strip_prefix('-')?.split(|c: char| c == '[' || c.is_whitespace()).next())
        .collect()
}

fn missing(required: &'static [&'static str], available: &[&str]) -> Vec<&'static str> {
    required.iter().copied().filter(|name| !available.contains(name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILTERS: &str = "Filters:
  T.. = Timeline support
  .S. = Slice threading
  ------
 TS blend             VV->V      Blend two video frames into each other.
 .. crop              V->V       Crop the input video.
 .S zscale            V->V       Apply resizing, colorspace and bit depth conversion.
";

    #[test]
    fn parses_version_token() {
        let out = "ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers\nbuilt with clang\n";
        assert_eq!(parse_version(out), Some("9.0.2"));
    }

    #[test]
    fn rejects_unknown_version_output() {
        assert_eq!(parse_version("hello\n"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn lists_names_after_separator_only() {
        assert_eq!(list_names(FILTERS), vec!["blend", "crop", "zscale"]);
    }

    #[test]
    fn reports_missing_required_names() {
        let available = ["libx264", "prores_ks"];
        assert_eq!(missing(REQUIRED_ENCODERS, &available), vec!["libx265"]);
        assert!(missing(REQUIRED_FILTERS, &list_names(FILTERS)).is_empty());
    }

    #[test]
    fn reports_missing_jxl_decoder() {
        let decoders = "Decoders:\n ------\n V....D 012v                 Uncompressed 4:2:2 10-bit\n V....D libjxl_anim          libjxl JPEG XL animated\n";
        assert_eq!(missing(REQUIRED_DECODERS, &list_names(decoders)), vec!["libjxl"]);
    }

    #[test]
    fn reports_missing_hdr_options() {
        let help = "Advanced options:\n-mastering_display[:<stream_spec>]  set SMPTE2084 mastering display color volume info\n";
        assert_eq!(missing(REQUIRED_OPTIONS, &list_options(help)), vec!["content_light"]);
    }

    #[test]
    fn override_that_does_not_exist_is_not_found() {
        let path = Path::new("/no/such/ffmpeg");
        assert!(matches!(locate(Some(path)), Err(BinaryError::NotFound(p)) if p == path));
    }
}
