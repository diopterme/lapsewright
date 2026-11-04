use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

/// `name.extension`. Not `Path::with_extension`, which would cut `2026.10.23` to `2026.10`.
pub fn file_name(name: &OsStr, extension: &str) -> OsString {
    let mut file = name.to_owned();
    file.push(".");
    file.push(extension);
    file
}

/// Creates an empty `dir/name.extension`, or `name_2`, `name_3`, … when taken.
/// Creating the file claims the name atomically, so deleting it after a failed
/// conversion can never remove someone else's file.
pub fn reserve(dir: &Path, name: &OsStr, extension: &str) -> io::Result<PathBuf> {
    for number in 1.. {
        let path = numbered(dir, name, extension, number);
        match File::create_new(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    unreachable!("a free name comes long before the numbers run out")
}

/// The name `reserve` would pick now (B2: shown before starting). Only looks; creates nothing.
pub fn next_free(dir: &Path, name: &OsStr, extension: &str) -> PathBuf {
    (1..).map(|number| numbered(dir, name, extension, number)).find(|path| !path.exists()).expect("a free name")
}

fn numbered(dir: &Path, name: &OsStr, extension: &str, number: u32) -> PathBuf {
    let mut stem = name.to_owned();
    if number > 1 {
        stem.push(format!("_{number}"));
    }
    dir.join(file_name(&stem, extension))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    #[test]
    fn dotted_names_keep_every_dot() {
        assert_eq!(file_name(OsStr::new("2026.10.23"), "mp4"), "2026.10.23.mp4");
    }

    #[test]
    fn reserve_numbers_taken_names_and_keeps_existing_files() {
        let dir = env::temp_dir().join(format!("lapsewright-reserve-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("day.mp4"), b"keep").unwrap();

        let second = reserve(&dir, OsStr::new("day"), "mp4").unwrap();
        let third = reserve(&dir, OsStr::new("day"), "mp4").unwrap();

        assert_eq!(second, dir.join("day_2.mp4"));
        assert_eq!(third, dir.join("day_3.mp4"));
        assert!(third.is_file());
        assert_eq!(fs::read(dir.join("day.mp4")).unwrap(), b"keep");
        assert_eq!(next_free(&dir, OsStr::new("day"), "mp4"), dir.join("day_4.mp4"));
        assert_eq!(next_free(&dir, OsStr::new("night"), "mp4"), dir.join("night.mp4"));
        assert!(!dir.join("day_4.mp4").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reserve_in_missing_folder_fails() {
        let dir = env::temp_dir().join("lapsewright-no-such-folder").join("x");
        assert!(reserve(&dir, OsStr::new("day"), "mp4").is_err());
    }
}
