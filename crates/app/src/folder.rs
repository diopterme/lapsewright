use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use lapsewright_core::settings::CROP_CENTER;
use lapsewright_core::{exif, source};
use serde::{Deserialize, Serialize};

use crate::lang::{is_ko, t};

const RECENT_MAX: usize = 5;
const RECENT_FILE: &str = "recent.json";
const RECENT_VERSION: u32 = 1;
const THUMBS_DIR: &str = "thumbs";

/// What the folder bar and the slider need.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub path: PathBuf,
    /// Where the video goes unless another folder was picked (C10).
    pub parent: Option<PathBuf>,
    pub name: String,
    /// File names in conversion order.
    pub names: Vec<String>,
    pub formats: Vec<String>,
    pub first_taken: Option<String>,
    pub last_taken: Option<String>,
    /// Where the video's frame sits (core `Settings::crop`), as last dragged in this folder.
    pub crop: u16,
}

/// A dropped photo opens the folder it is in.
pub fn open(path: &Path) -> Result<(Folder, Vec<PathBuf>), String> {
    let path = if path.is_file() { path.parent().unwrap_or(path) } else { path };
    let images = source::scan(path).map_err(|e| format!("{}: {e}", t("폴더를 못 읽음", "Can’t read the folder")))?;
    let (Some(first), Some(last)) = (images.first(), images.last()) else {
        return Err(t("사진이 없음 (JPG · PNG · JXL · TIF)", "No photos (JPG · PNG · JXL · TIF)").to_string());
    };
    // ffmpeg picks one decoder from the first photo, so a second format breaks conversion.
    let formats = source::formats(&images);
    if formats.len() > 1 {
        let formats = formats.join(" · ");
        if is_ko() {
            return Err(format!("형식이 섞여 있음 ({formats}). 한 가지 형식만 있는 폴더를 여세요"));
        }
        return Err(format!("Mixed formats ({formats}). Open a folder with one format only"));
    }
    let folder = Folder {
        path: path.to_path_buf(),
        parent: path.parent().map(Path::to_path_buf),
        name: file_name(path),
        names: images.iter().map(|p| file_name(p)).collect(),
        formats,
        first_taken: taken(first),
        last_taken: taken(last),
        crop: CROP_CENTER,
    };
    Ok((folder, images))
}

/// `2026-10-20T05:12:00`, as the camera wrote it.
pub fn taken(photo: &Path) -> Option<String> {
    exif::read(photo).ok()?.taken_at.map(|t| t.to_string())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Remembered {
    path: PathBuf,
    count: usize,
    /// Kept only while the folder is on the list (ponytail: a separate file if 5 is too few).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    crop: Option<u16>,
}

#[derive(Serialize, Deserialize)]
struct RecentFile {
    version: u32,
    folders: Vec<Remembered>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub path: PathBuf,
    pub name: String,
    pub count: usize,
    /// Moved or deleted folders stay listed, greyed out.
    pub exists: bool,
}

/// Newest first. A missing or unreadable file is an empty list: it only saves a few clicks.
pub fn recent(root: &Path) -> Vec<Recent> {
    load(root)
        .into_iter()
        .map(|r| Recent { name: file_name(&r.path), exists: r.path.is_dir(), path: r.path, count: r.count })
        .collect()
}

/// Puts the folder first and returns its crop.
pub fn remember(root: &Path, path: &Path, count: usize) -> io::Result<u16> {
    let old = load(root);
    let crop = old.iter().find(|r| r.path == path).and_then(|r| r.crop);
    let folders: Vec<Remembered> = std::iter::once(Remembered { path: path.to_path_buf(), count, crop })
        .chain(old.into_iter().filter(|r| r.path != path))
        .take(RECENT_MAX)
        .collect();
    save(root, folders)?;
    Ok(crop.unwrap_or(CROP_CENTER))
}

/// A folder off the list is not added: it was opened before the list was cleared.
pub fn set_crop(root: &Path, path: &Path, crop: u16) -> io::Result<()> {
    let folders = load(root)
        .into_iter()
        .map(|r| if r.path == path { Remembered { crop: Some(crop), ..r } } else { r })
        .collect();
    save(root, folders)
}

fn save(root: &Path, folders: Vec<Remembered>) -> io::Result<()> {
    fs::create_dir_all(root)?;
    prune_thumbs(root, &folders);
    let text = serde_json::to_string_pretty(&RecentFile { version: RECENT_VERSION, folders })?;
    fs::write(root.join(RECENT_FILE), text)
}

/// Y10: where a recent folder's card picture is kept. Named by the folder path, so a folder whose
/// first photo changed keeps its old picture (ponytail: add the photo's mtime to the name if that matters).
pub fn thumb_file(root: &Path, folder: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    folder.hash(&mut hasher);
    root.join(THUMBS_DIR).join(format!("{:016x}.jpg", hasher.finish()))
}

/// Pictures of folders that dropped off the list. Left behind ones only take disk space.
fn prune_thumbs(root: &Path, kept: &[Remembered]) {
    let keep: Vec<PathBuf> = kept.iter().map(|r| thumb_file(root, &r.path)).collect();
    let Ok(entries) = fs::read_dir(root.join(THUMBS_DIR)) else { return };
    for path in entries.flatten().map(|e| e.path()).filter(|p| !keep.contains(p)) {
        let _ = fs::remove_file(path);
    }
}

/// W4. Already gone is fine.
pub fn forget(root: &Path) -> io::Result<()> {
    prune_thumbs(root, &[]);
    match fs::remove_file(root.join(RECENT_FILE)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn load(root: &Path) -> Vec<Remembered> {
    fs::read_to_string(root.join(RECENT_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<RecentFile>(&text).ok())
        .filter(|file| file.version == RECENT_VERSION)
        .map_or_else(Vec::new, |file| file.folders)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lapsewright-app-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn recent_keeps_five_newest_without_repeats() {
        let root = temp("recent");
        for i in 0..7 {
            remember(&root, Path::new(&format!("/f{i}")), i).unwrap();
        }
        remember(&root, Path::new("/f4"), 40).unwrap();
        let list = recent(&root);
        let paths: Vec<_> = list.iter().map(|r| r.path.to_str().unwrap()).collect();
        assert_eq!(paths, ["/f4", "/f6", "/f5", "/f3", "/f2"]);
        assert_eq!((list[0].count, list[0].name.as_str(), list[0].exists), (40, "f4", false));
        forget(&root).unwrap();
        assert!(recent(&root).is_empty());
        forget(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn crop_stays_with_its_folder() {
        let root = temp("crop");
        assert_eq!(remember(&root, Path::new("/a"), 1).unwrap(), CROP_CENTER);
        remember(&root, Path::new("/b"), 2).unwrap();
        set_crop(&root, Path::new("/a"), 120).unwrap();
        assert_eq!(remember(&root, Path::new("/b"), 2).unwrap(), CROP_CENTER);
        assert_eq!(remember(&root, Path::new("/a"), 1).unwrap(), 120);
        set_crop(&root, Path::new("/gone"), 7).unwrap();
        assert_eq!(recent(&root).len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pictures_go_with_their_folders() {
        let root = temp("thumbs");
        let thumb = |i: usize| thumb_file(&root, Path::new(&format!("/f{i}")));
        fs::create_dir_all(root.join(THUMBS_DIR)).unwrap();
        for i in 0..6 {
            fs::write(thumb(i), b"jpg").unwrap();
            remember(&root, Path::new(&format!("/f{i}")), i).unwrap();
        }
        assert!(!thumb(0).exists() && thumb(1).exists() && thumb(5).exists());
        assert_ne!(thumb(1), thumb(2));
        forget(&root).unwrap();
        assert!(!thumb(5).exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn broken_recent_file_reads_as_empty() {
        let root = temp("broken");
        fs::write(root.join(RECENT_FILE), "{").unwrap();
        assert!(recent(&root).is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dropped_photo_opens_its_folder() {
        let dir = temp("open");
        for name in ["b.JPEG", "a.jpg", "c.jpg", "notes.txt"] {
            fs::write(dir.join(name), b"").unwrap();
        }
        let (folder, images) = open(&dir.join("c.jpg")).unwrap();
        assert_eq!(folder.path, dir);
        assert_eq!(folder.parent.as_deref(), dir.parent());
        assert_eq!(folder.names, ["a.jpg", "b.JPEG", "c.jpg"]);
        assert_eq!(folder.formats, ["JPG"]);
        assert_eq!(images.len(), 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mixed_formats_are_refused() {
        let dir = temp("mixed");
        for name in ["1.jpg", "2.jxl", "3.png"] {
            fs::write(dir.join(name), b"").unwrap();
        }
        assert_eq!(open(&dir).unwrap_err(), "형식이 섞여 있음 (JPG · JXL · PNG). 한 가지 형식만 있는 폴더를 여세요");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_without_photos_is_an_error() {
        let dir = temp("empty");
        assert_eq!(open(&dir).unwrap_err(), "사진이 없음 (JPG · PNG · JXL · TIF)");
        fs::remove_dir_all(&dir).unwrap();
    }
}
