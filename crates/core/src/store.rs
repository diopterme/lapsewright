use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::{CROP_CENTER, Chroma, Codec, DynamicRange, MAX_CRF, MotionBlur, Profile, Resolution, Settings};

const FORMAT_VERSION: u32 = 1;
/// Keeps `<id>.json` well inside every file system's name limit.
const MAX_ID_LEN: usize = 64;
/// Windows opens these as devices whatever the extension, so `nul.json` would save nowhere.
const WINDOWS_DEVICES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    pub settings: Settings,
}

/// The app data folder. Presets live in `presets/<id>.json`; the id is the file name.
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// `~/Library/Application Support/Lapsewright` on macOS, `%APPDATA%\Lapsewright` on Windows.
    /// `None` when the system reports no home folder.
    pub fn default_root() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("Lapsewright"))
    }

    pub fn new(root: PathBuf) -> Self {
        Store { root }
    }

    /// Every preset file, sorted by name. A file that can't be used is listed with its error,
    /// so it doesn't silently vanish.
    pub fn presets(&self) -> Result<Vec<(String, Result<Preset, StoreError>)>, StoreError> {
        let dir = self.presets_dir()?;
        let mut presets: Vec<_> = json_ids(&dir)?.into_iter().map(|id| { let preset = self.preset(&id); (id, preset) }).collect();
        presets.sort_by_key(|(id, preset)| preset.as_ref().map_or(id, |p| &p.name).to_lowercase());
        Ok(presets)
    }

    pub fn preset(&self, id: &str) -> Result<Preset, StoreError> {
        check_id(id)?;
        let path = self.presets_dir()?.join(format!("{id}.json"));
        let text = fs::read_to_string(&path).map_err(|e| not_found_or_io(id, &path, e))?;
        let file: PresetFile = serde_json::from_str(&text).map_err(|e| StoreError::Parse(path.clone(), e))?;
        // Unknown fields are ignored, so a file from another program would otherwise load with defaults.
        if file.version != FORMAT_VERSION {
            return Err(StoreError::Invalid(path, format!("not a version {FORMAT_VERSION} preset file")));
        }
        let settings = file.settings.to_settings().map_err(|reason| StoreError::Invalid(path, reason))?;
        Ok(Preset { name: file.name, settings })
    }

    /// Replaces a preset with the same id. A blank name is saved as the id, so no list row is empty.
    pub fn save_preset(&self, id: &str, preset: &Preset) -> Result<(), StoreError> {
        check_id(id)?;
        let name = match preset.name.trim() {
            "" => id,
            name => name,
        };
        write(&self.presets_dir()?, id, &Preset { name: name.to_string(), settings: preset.settings })
    }

    pub fn delete_preset(&self, id: &str) -> Result<(), StoreError> {
        check_id(id)?;
        let path = self.presets_dir()?.join(format!("{id}.json"));
        fs::remove_file(&path).map_err(|e| not_found_or_io(id, &path, e))
    }

    /// Brings back deleted built-ins. A built-in that was changed (or broke) is replaced only with
    /// `overwrite`; otherwise its name is returned, so the caller can ask first.
    pub fn restore_built_ins(&self, overwrite: bool) -> Result<Vec<String>, StoreError> {
        let dir = self.presets_dir()?;
        let mut kept = Vec::new();
        for (id, built_in) in built_ins() {
            match self.preset(id) {
                Ok(current) if current == built_in => {}
                Err(StoreError::NotFound(_)) => write(&dir, id, &built_in)?,
                _ if overwrite => write(&dir, id, &built_in)?,
                _ => kept.push(built_in.name),
            }
        }
        Ok(kept)
    }

    /// Creates the folder, and fills it with the built-ins when it holds no preset at all.
    fn presets_dir(&self) -> Result<PathBuf, StoreError> {
        let dir = self.root.join("presets");
        fs::create_dir_all(&dir).map_err(|e| StoreError::Io(dir.clone(), e))?;
        if json_ids(&dir)?.is_empty() {
            for (id, preset) in built_ins() {
                write(&dir, id, &preset)?;
            }
        }
        Ok(dir)
    }
}

fn built_ins() -> [(&'static str, Preset); 3] {
    let default = Settings::default();
    let preset = |name: &str, settings| Preset { name: name.to_string(), settings };
    [
        ("stock_4k_sdr", preset("Stock 4K SDR", default)),
        (
            "stock_4k_hdr_prores",
            preset(
                "Stock 4K HDR ProRes",
                Settings { profile: Profile::ProResHq, chroma: Chroma::Yuv422, range: DynamicRange::Hdr, ..default },
            ),
        ),
        (
            "preview_1080p",
            preset(
                "Preview 1080p",
                Settings {
                    profile: Profile::H264Main,
                    resolution: Resolution::new(1920, 1080).expect("even size"),
                    crf: 23,
                    blur: MotionBlur::Off,
                    ..default
                },
            ),
        ),
    ]
}

/// Letters, digits, `_` and `-` only, so the id can't leave the folder or change the extension.
fn check_id(id: &str) -> Result<(), StoreError> {
    let is_safe = !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !WINDOWS_DEVICES.contains(&id.to_ascii_lowercase().as_str());
    if is_safe { Ok(()) } else { Err(StoreError::InvalidId(id.to_string())) }
}

fn json_ids(dir: &Path) -> Result<Vec<String>, StoreError> {
    let io_error = |e| StoreError::Io(dir.to_path_buf(), e);
    let mut ids = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        let stem = path.file_stem().and_then(|s| s.to_str());
        if let (Some(stem), true) = (stem, path.extension().is_some_and(|e| e == "json")) {
            ids.push(stem.to_string());
        }
    }
    Ok(ids)
}

/// Written next to the target and renamed over it, so a crash never leaves half a file.
fn write(dir: &Path, id: &str, preset: &Preset) -> Result<(), StoreError> {
    let file = PresetFile { version: FORMAT_VERSION, name: preset.name.clone(), settings: preset.settings.into() };
    let json = serde_json::to_string_pretty(&file).expect("plain fields always serialize");
    let (temp, path) = (dir.join(format!("{id}.json.tmp")), dir.join(format!("{id}.json")));
    fs::write(&temp, json).map_err(|e| StoreError::Io(temp.clone(), e))?;
    fs::rename(&temp, &path).map_err(|e| StoreError::Io(path, e))
}

fn not_found_or_io(id: &str, path: &Path, e: io::Error) -> StoreError {
    if e.kind() == io::ErrorKind::NotFound { StoreError::NotFound(id.to_string()) } else { StoreError::Io(path.to_path_buf(), e) }
}

#[derive(Serialize, Deserialize)]
struct PresetFile {
    #[serde(default)]
    version: u32,
    name: String,
    #[serde(default)]
    settings: SettingsFile,
}

/// Plain values on disk and in the app's panel; checked when turned back into `Settings`.
/// Missing fields take the default.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsFile {
    pub codec: Codec,
    pub profile: String,
    pub chroma: Chroma,
    pub range: DynamicRange,
    pub width: u32,
    pub height: u32,
    pub crf: u8,
    pub fps: String,
    pub blur: u32,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Settings::default().into()
    }
}

impl From<Settings> for SettingsFile {
    fn from(s: Settings) -> Self {
        SettingsFile {
            codec: s.profile.codec(),
            profile: s.profile.spec().id.to_string(),
            chroma: s.chroma,
            range: s.range,
            width: s.resolution.width(),
            height: s.resolution.height(),
            crf: s.crf,
            fps: s.fps.to_string(),
            blur: s.blur.frames(),
        }
    }
}

impl SettingsFile {
    pub fn to_settings(&self) -> Result<Settings, String> {
        let profile = Profile::find(self.codec, &self.profile)
            .ok_or_else(|| format!("unknown profile `{}` for {:?}", self.profile, self.codec))?;
        if self.crf > MAX_CRF {
            return Err(format!("crf must be 0-{MAX_CRF}: {}", self.crf));
        }
        Ok(Settings {
            profile,
            chroma: self.chroma,
            range: self.range,
            resolution: Resolution::new(self.width, self.height).map_err(|e| e.to_string())?,
            crf: self.crf,
            fps: self.fps.parse().map_err(|e: crate::settings::SettingsError| e.to_string())?,
            blur: MotionBlur::nearest(self.blur),
            crop: CROP_CENTER,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("preset id must be 1-{MAX_ID_LEN} letters, digits, `_` or `-`, and not a Windows device name: `{0}`")]
    InvalidId(String),
    #[error("no preset `{0}`")]
    NotFound(String),
    #[error("{0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("{0}: {1}")]
    Parse(PathBuf, serde_json::Error),
    #[error("{0}: {1}")]
    Invalid(PathBuf, String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Chroma, DynamicRange, Fps, MotionBlur, Profile, Resolution};
    use std::{env, fs};

    fn store(name: &str) -> (Store, PathBuf) {
        let root = env::temp_dir().join(format!("lapsewright-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        (Store::new(root.clone()), root)
    }

    fn hdr_preset() -> Preset {
        let settings = Settings {
            profile: Profile::HevcMain422_10,
            chroma: Chroma::Yuv422,
            range: DynamicRange::Hlg,
            resolution: Resolution::new(3840, 2160).unwrap(),
            crf: 12,
            fps: "24000/1001".parse::<Fps>().unwrap(),
            blur: MotionBlur::Frames15,
            crop: CROP_CENTER,
        };
        Preset { name: "Night HLG".into(), settings }
    }

    fn ids(store: &Store) -> Vec<String> {
        store.presets().unwrap().into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn saved_preset_reads_back_the_same() {
        let (store, root) = store("roundtrip");
        store.save_preset("night", &hdr_preset()).unwrap();
        assert_eq!(store.preset("night").unwrap(), hdr_preset());
        let file = fs::read_to_string(root.join("presets/night.json")).unwrap();
        assert!(file.contains("\"version\": 1"), "{file}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blank_name_is_saved_as_the_id_and_names_are_trimmed() {
        let (store, root) = store("blank");
        store.save_preset("night", &Preset { name: "  ".into(), ..hdr_preset() }).unwrap();
        assert_eq!(store.preset("night").unwrap().name, "night");
        store.save_preset("day", &Preset { name: " Day ".into(), ..hdr_preset() }).unwrap();
        assert_eq!(store.preset("day").unwrap().name, "Day");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ids_that_could_escape_the_folder_or_name_a_device_are_rejected() {
        let (store, root) = store("ids");
        let long = "a".repeat(65);
        let bad = ["", "../../etc/passwd", "a/b", r"..\x", "a.json", "a b", "night\0", "con", "NUL", "Com1", "lpt9", &long];
        for id in bad {
            assert!(matches!(store.save_preset(id, &hdr_preset()), Err(StoreError::InvalidId(_))), "save {id:?}");
            assert!(matches!(store.preset(id), Err(StoreError::InvalidId(_))), "read {id:?}");
            assert!(matches!(store.delete_preset(id), Err(StoreError::InvalidId(_))), "delete {id:?}");
        }
        assert!(!root.parent().unwrap().join("etc").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn built_ins_fill_only_an_empty_folder() {
        let (store, root) = store("seed");
        assert_eq!(ids(&store), ["preview_1080p", "stock_4k_hdr_prores", "stock_4k_sdr"]);
        store.delete_preset("preview_1080p").unwrap();
        assert_eq!(ids(&store), ["stock_4k_hdr_prores", "stock_4k_sdr"]);
        store.delete_preset("stock_4k_hdr_prores").unwrap();
        store.delete_preset("stock_4k_sdr").unwrap();
        assert_eq!(ids(&store).len(), 3, "emptied folder is filled again");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restoring_adds_deleted_built_ins_and_replaces_changed_ones_only_when_asked() {
        let (store, root) = store("restore");
        store.delete_preset("preview_1080p").unwrap();
        store.save_preset("stock_4k_sdr", &Preset { name: "Mine".into(), ..hdr_preset() }).unwrap();
        store.save_preset("night", &hdr_preset()).unwrap();

        assert_eq!(store.restore_built_ins(false).unwrap(), ["Stock 4K SDR"]);
        assert_eq!(store.preset("preview_1080p").unwrap().name, "Preview 1080p");
        assert_eq!(store.preset("stock_4k_sdr").unwrap().name, "Mine");

        assert!(store.restore_built_ins(true).unwrap().is_empty());
        assert_eq!(store.preset("stock_4k_sdr").unwrap(), Preset { name: "Stock 4K SDR".into(), settings: Settings::default() });
        assert_eq!(store.preset("night").unwrap(), hdr_preset(), "own presets are left alone");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_is_sorted_by_name_and_shows_broken_files() {
        let (store, root) = store("list");
        store.save_preset("aaa", &Preset { name: "Zebra".into(), ..hdr_preset() }).unwrap();
        fs::write(root.join("presets/broken.json"), "{ not json").unwrap();
        fs::write(root.join("presets/notes.txt"), "ignored").unwrap();
        let listed = store.presets().unwrap();
        let names: Vec<_> = listed.iter().map(|(id, p)| p.as_ref().map_or(id.as_str(), |p| p.name.as_str())).collect();
        assert_eq!(names, ["broken", "Preview 1080p", "Stock 4K HDR ProRes", "Stock 4K SDR", "Zebra"]);
        assert!(listed[0].1.is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_settings_fields_come_from_the_default() {
        let (store, root) = store("defaults");
        store.save_preset("x", &hdr_preset()).unwrap();
        let json = r#"{ "version": 1, "name": "Old", "settings": { "codec": "prores", "profile": "4444" } }"#;
        fs::write(root.join("presets/old.json"), json).unwrap();
        let expected = Settings { profile: Profile::ProRes4444, ..Settings::default() };
        assert_eq!(store.preset("old").unwrap(), Preset { name: "Old".into(), settings: expected });
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_without_this_format_version_is_unusable_not_misread() {
        let (store, root) = store("version");
        store.save_preset("x", &hdr_preset()).unwrap();
        let other_app = r#"{ "id": "p", "name": "Preview 1080p", "settings": { "resolution": { "width": 1920, "height": 1080 }, "codec": "h264", "profileId": "main" } }"#;
        fs::write(root.join("presets/p.json"), other_app).unwrap();
        assert!(matches!(store.preset("p"), Err(StoreError::Invalid(..))));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_values_in_a_file_are_errors() {
        let (store, root) = store("invalid");
        store.save_preset("x", &hdr_preset()).unwrap();
        let cases = [
            ("profile", r#"{ "codec": "h264", "profile": "main10" }"#),
            ("size", r#"{ "width": 1921 }"#),
            ("fps", r#"{ "fps": "29.97" }"#),
            ("crf", r#"{ "crf": 52 }"#),
            ("codec", r#"{ "codec": "av1" }"#),
        ];
        for (id, settings) in cases {
            fs::write(root.join(format!("presets/{id}.json")), format!(r#"{{ "version": 1, "name": "n", "settings": {settings} }}"#)).unwrap();
            assert!(store.preset(id).is_err(), "{id}");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_preset_is_not_found() {
        let (store, root) = store("missing");
        assert!(matches!(store.preset("nope"), Err(StoreError::NotFound(_))));
        assert!(matches!(store.delete_preset("nope"), Err(StoreError::NotFound(_))));
        fs::remove_dir_all(root).unwrap();
    }
}
