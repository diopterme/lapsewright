use std::time::{SystemTime, UNIX_EPOCH};

use lapsewright_core::rules;
use lapsewright_core::settings::{Chroma, Codec, Profile};
use lapsewright_core::store::{Preset, SettingsFile, Store};
use serde::Serialize;
use tauri::State;

use crate::App;
use crate::lang::t;

/// What the profile menu and the greyed-out choices need (C2, D5).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    codec: Codec,
    id: &'static str,
    bits: u8,
    chroma: &'static [Chroma],
    hdr: bool,
    /// Picked when the use (= codec) changes (C3).
    is_default: bool,
    extension: &'static str,
}

#[derive(Serialize)]
pub struct PresetEntry {
    id: String,
    name: String,
    /// `None` for a file that can't be used; `error` says why.
    settings: Option<SettingsFile>,
    error: Option<String>,
}

#[tauri::command]
pub fn profiles() -> Vec<ProfileInfo> {
    Profile::ALL
        .into_iter()
        .map(|p| {
            let spec = p.spec();
            ProfileInfo {
                codec: spec.codec,
                id: spec.id,
                bits: spec.bits,
                chroma: spec.chroma,
                hdr: spec.hdr,
                is_default: spec.codec.default_profile() == p,
                extension: spec.codec.extension(),
            }
        })
        .collect()
}

#[tauri::command(async)]
pub fn presets(app: State<App>) -> Result<Vec<PresetEntry>, String> {
    let list = store(&app)?.presets().map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|(id, preset)| match preset {
            Ok(p) => PresetEntry { name: p.name, settings: Some(p.settings.into()), error: None, id },
            Err(e) => PresetEntry { name: id.clone(), settings: None, error: Some(e.to_string()), id },
        })
        .collect())
}

/// Replaces preset `id`, or makes a new one without it. Returns the id.
#[tauri::command(async)]
pub fn save_preset(id: Option<String>, name: String, settings: SettingsFile, app: State<App>) -> Result<String, String> {
    let settings = rules::normalize(settings.to_settings()?);
    // Names may be Korean; file names stay ASCII.
    let id = id.unwrap_or_else(|| format!("p{}", SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())));
    store(&app)?.save_preset(&id, &Preset { name, settings }).map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command(async)]
pub fn delete_preset(id: String, app: State<App>) -> Result<(), String> {
    store(&app)?.delete_preset(&id).map_err(|e| e.to_string())
}

/// Names of changed built-ins left as they are; ask, then call again with `overwrite` (D6).
#[tauri::command(async)]
pub fn restore_built_ins(overwrite: bool, app: State<App>) -> Result<Vec<String>, String> {
    store(&app)?.restore_built_ins(overwrite).map_err(|e| e.to_string())
}

fn store(app: &App) -> Result<Store, String> {
    app.data.clone().map(Store::new).ok_or_else(|| t("앱 데이터 폴더가 없음 (홈 폴더를 못 찾음)", "No app data folder (home folder not found)").to_string())
}
