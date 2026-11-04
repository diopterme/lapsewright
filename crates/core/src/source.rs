use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "jxl", "tif", "tiff"];

/// Images directly in `folder` (no subfolders), in natural order of their file names.
pub fn scan(folder: &Path) -> io::Result<Vec<PathBuf>> {
    let entries: Vec<PathBuf> = fs::read_dir(folder)?.map(|e| e.map(|e| e.path())).collect::<io::Result<_>>()?;
    let mut images: Vec<PathBuf> = entries.into_iter().filter(|p| is_image(p) && p.is_file()).collect();
    images.sort_by(|a, b| natural_cmp(&file_name(a), &file_name(b)).then_with(|| a.cmp(b)));
    Ok(images)
}

/// Upper-case names, sorted; `jpeg` counts as `JPG`, `tiff` as `TIF`.
pub fn formats(images: &[PathBuf]) -> Vec<String> {
    let set: BTreeSet<String> = images
        .iter()
        .filter_map(|p| p.extension()?.to_str())
        .map(|e| match e.to_ascii_uppercase().as_str() {
            "JPEG" => "JPG".to_string(),
            "TIFF" => "TIF".to_string(),
            other => other.to_string(),
        })
        .collect();
    set.into_iter().collect()
}

/// ffmpeg 9.0.2 hangs on a few bytes named `.jxl` and quietly ends the video at an empty or random
/// file, so every photo must start the way its format does (measured: 281 photos on a NAS, 0.8 s).
pub fn check(photo: &Path) -> io::Result<()> {
    let mut head = Vec::new();
    File::open(photo)?.take(12).read_to_end(&mut head)?;
    let extension = photo.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_uppercase();
    let signatures: &[&[u8]] = match extension.as_str() {
        "JPG" | "JPEG" => &[b"\xff\xd8\xff"],
        "PNG" => &[b"\x89PNG\r\n\x1a\n"],
        "JXL" => &[b"\xff\x0a", b"\0\0\0\x0cJXL \r\n\x87\n"],
        // Little- and big-endian.
        "TIF" | "TIFF" => &[b"II*\0", b"MM\0*"],
        _ => &[],
    };
    if signatures.iter().any(|s| head.starts_with(s)) {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, format!("not a {extension} photo")))
    }
}

/// Dot files are skipped: macOS leaves `._IMG_0001.jpg` resource forks on network and external drives.
fn is_image(path: &Path) -> bool {
    let is_hidden = file_name(path).starts_with('.');
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    !is_hidden && EXTENSIONS.iter().any(|ext| ext.eq_ignore_ascii_case(extension))
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Digit runs compare as numbers (without parsing, so any length works): `IMG_9` < `IMG_10`.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        let (Some(x), Some(y)) = (a.chars().next(), b.chars().next()) else {
            return a.len().cmp(&b.len());
        };
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let (number_a, rest_a) = split_digits(a);
            let (number_b, rest_b) = split_digits(b);
            let (number_a, number_b) = (number_a.trim_start_matches('0'), number_b.trim_start_matches('0'));
            let order = number_a.len().cmp(&number_b.len()).then_with(|| number_a.cmp(number_b));
            if order != Ordering::Equal {
                return order;
            }
            (a, b) = (rest_a, rest_b);
        } else if x != y {
            return x.cmp(&y);
        } else {
            (a, b) = (&a[x.len_utf8()..], &b[y.len_utf8()..]);
        }
    }
}

fn split_digits(s: &str) -> (&str, &str) {
    s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn digit_runs_compare_as_numbers() {
        let mut names = ["IMG_10.jpg", "IMG_9.jpg", "IMG_100.jpg", "IMG_1.jpg", "IMG_09b.jpg", "a2b10", "a2b9"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["IMG_1.jpg", "IMG_9.jpg", "IMG_09b.jpg", "IMG_10.jpg", "IMG_100.jpg", "a2b9", "a2b10"]);
    }

    #[test]
    fn leading_zeros_do_not_change_the_number() {
        assert_eq!(natural_cmp("7S207158", "7S0207159"), Ordering::Less);
        assert_eq!(natural_cmp("x01", "x1"), Ordering::Equal);
    }

    #[test]
    fn huge_digit_runs_do_not_overflow() {
        assert_eq!(natural_cmp("99999999999999999999999", "100000000000000000000000"), Ordering::Less);
    }

    #[test]
    fn check_wants_the_format_signature() {
        let dir = env::temp_dir().join(format!("lapsewright-check-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let files: [(&str, &[u8]); 10] = [
            ("a.JPG", b"\xff\xd8\xff\xe0rest"),
            ("b.png", b"\x89PNG\r\n\x1a\n"),
            ("c.jxl", b"\xff\x0a"),
            ("d.jxl", b"\0\0\0\x0cJXL \r\n\x87\nrest"),
            ("junk.jxl", b"junk\n"),
            ("empty.jpg", b""),
            ("png.jpg", b"\x89PNG\r\n\x1a\n"),
            ("e.tif", b"II*\0rest"),
            ("f.TIFF", b"MM\0*"),
            ("junk.tif", b"II*x"),
        ];
        for (name, bytes) in files {
            fs::write(dir.join(name), bytes).unwrap();
        }
        let ok: Vec<bool> = files.iter().map(|(name, _)| check(&dir.join(name)).is_ok()).collect();
        assert_eq!(ok, [true, true, true, true, false, false, false, true, true, false]);
        assert_eq!(check(&dir.join("junk.jxl")).unwrap_err().to_string(), "not a JXL photo");
        assert_eq!(check(&dir.join("gone.jpg")).unwrap_err().kind(), io::ErrorKind::NotFound);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scan_keeps_only_images_in_the_folder() {
        let dir = env::temp_dir().join(format!("lapsewright-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub.jpg")).unwrap();
        for name in ["b10.JPG", "b9.jpg", "a.png", "c.jxl", "d.jpeg", "e.tif", "f.TIFF", "notes.txt", "._b9.jpg", ".hidden.jpg", "noext"] {
            fs::write(dir.join(name), b"").unwrap();
        }
        let images = scan(&dir).unwrap();
        let names: Vec<_> = images.iter().map(|p| p.file_name().unwrap().to_owned()).collect();
        assert_eq!(names, ["a.png", "b9.jpg", "b10.JPG", "c.jxl", "d.jpeg", "e.tif", "f.TIFF"]);
        assert_eq!(formats(&images), ["JPG", "JXL", "PNG", "TIF"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
