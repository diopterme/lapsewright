use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

const PREFS_FILE: &str = "settings.json";
const PREFS_VERSION: u32 = 1;

/// The settings window (④). Empty author or holder: the photo's EXIF is used (M2).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub author: String,
    pub copyright: String,
    /// F4: once the queue is done, with the last video.
    pub reveal: bool,
    pub open: bool,
    pub power: Power,
}

/// F4: sleep and shut down can't both happen, so it is one choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Power {
    #[default]
    None,
    Sleep,
    Shutdown,
}

#[derive(Serialize, Deserialize)]
struct PrefsFile<P> {
    version: u32,
    #[serde(flatten)]
    prefs: P,
}

/// A missing or unreadable file is the default; the window shows what is in use.
pub fn load(root: &Path) -> Prefs {
    fs::read_to_string(root.join(PREFS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<PrefsFile<Prefs>>(&text).ok())
        .filter(|file| file.version == PREFS_VERSION)
        .map_or_else(Prefs::default, |file| file.prefs)
}

pub fn save(root: &Path, prefs: &Prefs) -> io::Result<()> {
    fs::create_dir_all(root)?;
    let text = serde_json::to_string_pretty(&PrefsFile { version: PREFS_VERSION, prefs })?;
    fs::write(root.join(PREFS_FILE), text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lapsewright-prefs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saved_prefs_come_back() {
        let root = temp("roundtrip");
        let prefs = Prefs { author: "Kim".into(), copyright: "Kim Studio".into(), reveal: true, open: false, power: Power::Sleep };
        save(&root, &prefs).unwrap();
        assert_eq!(load(&root), prefs);
        assert!(fs::read_to_string(root.join(PREFS_FILE)).unwrap().contains(r#""version": 1"#));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_broken_or_other_version_is_the_default() {
        let root = temp("broken");
        assert_eq!(load(&root), Prefs::default());
        fs::write(root.join(PREFS_FILE), "{").unwrap();
        assert_eq!(load(&root), Prefs::default());
        fs::write(root.join(PREFS_FILE), r#"{ "version": 2, "author": "Kim" }"#).unwrap();
        assert_eq!(load(&root), Prefs::default());
        // Fields added later read as their default.
        fs::write(root.join(PREFS_FILE), r#"{ "version": 1, "author": "Kim" }"#).unwrap();
        assert_eq!(load(&root), Prefs { author: "Kim".into(), ..Prefs::default() });
        fs::remove_dir_all(&root).unwrap();
    }
}
