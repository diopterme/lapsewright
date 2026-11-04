use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::builder;
use crate::convert::{LinkError, Sequence};
use crate::runner::{self, Canceller, Job, RunError};
use crate::settings::{MotionBlur, Settings};

/// Longest side of a preview. Measured on 22 MP photos: JPG 0.16 s, JXL 0.75 s.
pub const SIDE: u32 = 1280;

static NEXT_OUTPUT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("no photo number {0}")]
    NoImage(usize),
    #[error(transparent)]
    Link(#[from] LinkError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("reading the preview: {0}")]
    Read(io::Error),
}

/// One preview being made. `wait` returns the JPEG.
pub struct Preview {
    job: Job,
    output: TempFile,
    _sequence: Sequence,
}

/// Photo `index` as the video would show it: same frame rate and motion blur as the conversion,
/// but the whole photo (the screen draws the crop) and at most `SIDE` pixels.
pub fn start(ffmpeg: &Path, images: &[PathBuf], index: usize, settings: &Settings) -> Result<Preview, PreviewError> {
    let frames = frames(images, index, settings.blur).ok_or(PreviewError::NoImage(index))?;
    let sequence = Sequence::link(&frames)?;
    let id = NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed);
    let output = TempFile(env::temp_dir().join(format!("lapsewright-preview-{}-{id}.jpg", process::id())));
    let job = runner::start(ffmpeg, args(settings, &sequence.pattern, &output.0))?;
    Ok(Preview { job, output, _sequence: sequence })
}

impl Preview {
    pub fn canceller(&self) -> Canceller {
        self.job.canceller()
    }

    pub fn wait(self) -> Result<Vec<u8>, PreviewError> {
        self.job.wait()?;
        fs::read(&self.output.0).map_err(PreviewError::Read)
    }
}

struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// The photo and, with blur, the n-1 after it. Near the end the last photo repeats, as in
/// the conversion, where each blend input keeps showing its last frame (`eof_action=repeat`).
fn frames(images: &[PathBuf], index: usize, blur: MotionBlur) -> Option<Vec<PathBuf>> {
    let last = images.len().checked_sub(1)?;
    if index > last {
        return None;
    }
    let count = blur.frames().max(1) as usize;
    Some((0..count).map(|i| images[(index + i).min(last)].clone()).collect())
}

fn args(settings: &Settings, photos: &Path, output: &Path) -> Vec<OsString> {
    let fps = settings.fps.to_string();
    let scale = format!("scale=w='if(gte(iw,ih),{SIDE},-2)':h='if(gte(iw,ih),-2,{SIDE})'");
    let chain = [Some(scale), builder::blend_chain(settings.blur)].into_iter().flatten().collect::<Vec<_>>().join(",");
    // `-xerror`: a truncated JXL otherwise makes ffmpeg retry the same packet forever.
    let head = ["-hide_banner", "-v", "error", "-xerror", "-y", "-r", &fps].into_iter().chain(builder::SEQUENCE_INPUT);
    let tail = ["-filter_complex", &chain, "-frames:v", "1", "-c:v", "mjpeg", "-pix_fmt", "yuvj420p", "-q:v", "3", "-update", "1"];
    head.map(OsString::from)
        .chain([photos.as_os_str().to_owned()])
        .chain(tail.iter().map(OsString::from))
        .chain([output.as_os_str().to_owned()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn blur_reads_the_next_photos_and_repeats_the_last() {
        let images = paths(&["a", "b", "c", "d", "e", "f"]);
        assert_eq!(frames(&images, 0, MotionBlur::Frames5), Some(paths(&["a", "b", "c", "d", "e"])));
        assert_eq!(frames(&images, 4, MotionBlur::Frames5), Some(paths(&["e", "f", "f", "f", "f"])));
        assert_eq!(frames(&images, 2, MotionBlur::Off), Some(paths(&["c"])));
    }

    #[test]
    fn index_past_the_end_has_no_frames() {
        assert_eq!(frames(&paths(&["a"]), 1, MotionBlur::Off), None);
        assert_eq!(frames(&[], 0, MotionBlur::Off), None);
    }

    #[test]
    fn args_scale_then_blend_into_one_jpeg() {
        let args = args(&Settings::default(), Path::new("/tmp/x/%06d.jpg"), Path::new("/tmp/p.jpg"));
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        let chain = args[args.iter().position(|a| *a == "-filter_complex").unwrap() + 1];
        assert!(chain.starts_with("scale=w='if(gte(iw,ih),1280,-2)':h='if(gte(iw,ih),-2,1280)',split=5[s0]"));
        let head = ["-hide_banner", "-v", "error", "-xerror", "-y", "-r", "30000/1001", "-f", "image2", "-pattern_type", "sequence", "-start_number", "1", "-i"];
        assert_eq!(args[..head.len()], head);
        assert_eq!(args[head.len()], "/tmp/x/%06d.jpg");
        assert_eq!(args.last(), Some(&"/tmp/p.jpg"));
    }
}
