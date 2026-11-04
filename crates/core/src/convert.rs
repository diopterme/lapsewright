use std::env;
use std::fs;
use std::io;
use std::path::{self, Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::builder;
use crate::metadata::VideoMetadata;
use crate::progress::Progress;
use crate::quicktime;
use crate::runner::{self, Canceller, RunError};
use crate::settings::Settings;
use crate::source;

static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("no images to convert")]
    NoImages,
    #[error(transparent)]
    Link(#[from] LinkError),
    /// ffmpeg ends the video early, and still exits 0, at an empty photo or a missing last one.
    #[error("the video stopped at frame {made} of {expected}: a photo could not be read")]
    Short { made: u64, expected: usize, log: Vec<String> },
    #[error(transparent)]
    Run(#[from] RunError),
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    /// Missing, unreadable, or not starting like its format (`source::check`).
    #[error("{}: {source}", photo.display())]
    Photo { photo: PathBuf, source: io::Error },
    #[error("linking the photos: {0}")]
    Io(#[from] io::Error),
}

/// A finished video.
#[derive(Debug)]
pub struct Converted {
    /// What ffmpeg wrote to stderr.
    pub log: Vec<String>,
    /// `Some`: the video plays, but its Apple keys could not be written.
    pub key_error: Option<io::Error>,
}

/// Runs one conversion to the end. `output` belongs to this call (see `output::reserve`):
/// it is deleted unless the conversion succeeds. `metadata` comes from `metadata::build`.
pub fn run(
    ffmpeg: &Path,
    settings: &Settings,
    images: &[PathBuf],
    metadata: &VideoMetadata,
    output: &Path,
    on_start: impl FnOnce(Canceller),
    on_progress: impl FnMut(Progress),
) -> Result<Converted, ConvertError> {
    let log = encode(ffmpeg, settings, images, metadata, output, on_start, on_progress).inspect_err(|_| {
        // A leftover partial file is the lesser problem; the conversion error is what gets reported.
        let _ = fs::remove_file(output);
    })?;
    // The video plays without the Apple keys, so failing to add them doesn't cost the render.
    Ok(Converted { log, key_error: quicktime::add_keys(output, &metadata.keys).err() })
}

fn encode(
    ffmpeg: &Path,
    settings: &Settings,
    images: &[PathBuf],
    metadata: &VideoMetadata,
    output: &Path,
    on_start: impl FnOnce(Canceller),
    mut on_progress: impl FnMut(Progress),
) -> Result<Vec<String>, ConvertError> {
    if images.is_empty() {
        return Err(ConvertError::NoImages);
    }
    let sequence = Sequence::link(images)?;
    let job = runner::start(ffmpeg, builder::build(settings, &sequence.pattern, images.len(), metadata, output))?;
    on_start(job.canceller());
    let mut made = 0;
    job.progress.iter().for_each(|p| {
        made = p.frame.unwrap_or(made);
        on_progress(p);
    });
    let log = job.wait()?;
    if made < images.len() as u64 {
        return Err(ConvertError::Short { made, expected: images.len(), log });
    }
    Ok(log)
}

/// Numbered links to the photos (`000001.jxl` …), read as one image2 sequence. The concat
/// demuxer decodes one photo at a time; a sequence decodes several at once
/// (measured, JXL 6240×3510 on M1: 1.8 → 8 photos/s; 4K ProRes HDR 1.5 → 4 frames/s).
pub(crate) struct Sequence {
    dir: PathBuf,
    /// For ffmpeg's `-i`, numbered from 1.
    pub(crate) pattern: PathBuf,
}

impl Sequence {
    /// The folder holds one format (`source::formats`), so the first photo's extension names them all.
    pub(crate) fn link(images: &[PathBuf]) -> Result<Sequence, LinkError> {
        for photo in images {
            source::check(photo).map_err(|source| LinkError::Photo { photo: photo.clone(), source })?;
        }
        let id = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("lapsewright-{}-{id}", process::id()));
        let extension = images.first().and_then(|p| p.extension()).unwrap_or_default().to_ascii_lowercase();
        let extension = extension.to_str().ok_or_else(|| io::Error::other("photo extension is not UTF-8"))?;
        let text = dir.to_str().ok_or_else(|| io::Error::other("temp folder path is not UTF-8"))?;
        // `%` is ffmpeg's placeholder, so one in the temp path is doubled.
        let pattern = PathBuf::from(format!("{}/%06d.{extension}", text.replace('%', "%%")));
        fs::create_dir(&dir)?;
        // Owned before linking so a failed link still removes the folder.
        let sequence = Sequence { dir, pattern };
        for (i, image) in images.iter().enumerate() {
            symlink(&path::absolute(image)?, &sequence.dir.join(format!("{:06}.{extension}", i + 1)))?;
        }
        Ok(sequence)
    }
}

impl Drop for Sequence {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[cfg(unix)]
fn symlink(original: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

// ponytail: Windows needs Developer Mode or admin for symlinks; pick hard links or a pipe there.
#[cfg(windows)]
fn symlink(original: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_links_every_photo_in_order_and_goes_away() {
        let photos = env::temp_dir().join(format!("lapsewright-sequence-{}", process::id()));
        let _ = fs::remove_dir_all(&photos);
        fs::create_dir_all(&photos).unwrap();
        let images = [photos.join("IMG_9.JXL"), photos.join("it's 10.JXL")];
        images.iter().for_each(|p| fs::write(p, b"\xff\x0a").unwrap());
        let sequence = Sequence::link(&images).unwrap();
        let dir = sequence.dir.clone();
        assert_eq!(sequence.pattern, dir.join("%06d.jxl"));
        let targets: Vec<PathBuf> = (1..=2).map(|i| fs::read_link(dir.join(format!("{i:06}.jxl"))).unwrap()).collect();
        assert_eq!(targets, images);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        drop(sequence);
        assert!(!dir.exists());

        fs::write(&images[1], b"").unwrap();
        assert!(matches!(Sequence::link(&images), Err(LinkError::Photo { photo, .. }) if photo == images[1]));
        fs::remove_dir_all(&photos).unwrap();
    }
}
