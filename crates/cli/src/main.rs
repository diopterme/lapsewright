mod metadata_args;
mod settings_args;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use jiff::fmt::strtime::BrokenDownTime;
use lapsewright_core::convert::{self, ConvertError};
use lapsewright_core::progress::Progress;
use lapsewright_core::runner::RunError;
use lapsewright_core::exif::{self, ExifData};
use lapsewright_core::geocode::Geocoder;
use lapsewright_core::settings::Settings;
use lapsewright_core::store::{Preset, Store};
use lapsewright_core::{binaries, builder, output, rules, source};
use metadata_args::MetadataArgs;
use settings_args::SettingsArgs;

/// What a failed conversion prints of ffmpeg's stderr: the cause is near the end.
const STDERR_TAIL_LINES: usize = 20;

#[derive(Parser)]
#[command(name = "lapsewright", version)]
struct Cli {
    /// ffmpeg to use instead of the bundled one
    #[arg(long, global = true)]
    ffmpeg: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that ffmpeg has the filters and encoders lapsewright needs
    Check,
    /// Print the ffmpeg command for a folder without running it
    Args {
        /// Folder of photos; its name becomes the video name
        folder: PathBuf,
        /// Folder for the video
        #[arg(long, default_value = ".")]
        out: PathBuf,
        #[command(flatten)]
        settings: SettingsArgs,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Convert a folder of photos into a video; Ctrl+C cancels
    Convert {
        /// Folder of photos; its name becomes the video name
        folder: PathBuf,
        /// Folder for the video; an existing name gets `_2`, `_3`, …
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        settings: SettingsArgs,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Print the EXIF lapsewright reads from a photo
    Exif {
        photo: PathBuf,
    },
    /// Search a place by name (OpenStreetMap), up to five results
    Geocode {
        query: String,
    },
    /// Manage saved settings (presets)
    Preset {
        #[command(subcommand)]
        action: PresetAction,
    },
}

#[derive(Subcommand)]
enum PresetAction {
    /// List presets by name
    List,
    /// Print one preset
    Show { id: String },
    /// Save settings under an id (letters, digits, `_`, `-`); an existing id is replaced
    Save {
        id: String,
        /// Shown in lists; default: the id
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        settings: SettingsArgs,
    },
    /// Delete a preset
    Delete { id: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Check => check(cli.ffmpeg.as_deref()),
        Command::Args { folder, out, settings, metadata } => print_args(folder, out, settings, metadata),
        Command::Convert { folder, out, settings, metadata } => {
            run_convert(cli.ffmpeg.as_deref(), folder, out, settings, metadata)
        }
        Command::Exif { photo } => print_exif(photo),
        Command::Geocode { query } => geocode(query),
        Command::Preset { action } => preset(action),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn check(ffmpeg: Option<&Path>) -> Result<ExitCode, String> {
    let path = binaries::locate(ffmpeg).map_err(|e| e.to_string())?;
    let report = binaries::check(&path).map_err(|e| e.to_string())?;
    println!("ffmpeg   {}", report.path.display());
    println!("version  {}", report.version);
    print_items("filter", binaries::REQUIRED_FILTERS, &report.missing_filters);
    print_items("encoder", binaries::REQUIRED_ENCODERS, &report.missing_encoders);
    print_items("decoder", binaries::REQUIRED_DECODERS, &report.missing_decoders);
    print_items("option", binaries::REQUIRED_OPTIONS, &report.missing_options);
    Ok(if report.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn print_items(kind: &str, required: &[&str], missing: &[&str]) {
    for name in required {
        let status = if missing.contains(name) { "MISSING" } else { "ok" };
        println!("{kind:<8} {name:<10} {status}");
    }
}

fn print_args(folder: &Path, out: &Path, settings: &SettingsArgs, metadata: &MetadataArgs) -> Result<ExitCode, String> {
    let settings = settings.to_settings()?;
    let (folder, images) = scan(folder)?;
    let name = folder.file_name().ok_or("folder has no name")?;
    let output = out.join(output::file_name(name, settings.profile.codec().extension()));
    let metadata = metadata.build(&images[0])?;
    // The real photo links are made in a temp folder at convert time.
    let args = builder::build(&settings, Path::new("<photos>"), images.len(), &metadata, &output);
    println!("{}", builder::command_line(&args));
    Ok(ExitCode::SUCCESS)
}

/// The canonical folder (its name becomes the video name) and its photos, at least one.
fn scan(folder: &Path) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let folder = fs::canonicalize(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let images = source::scan(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    if images.is_empty() {
        return Err(format!("no {} images in {}", source::EXTENSIONS.join("/"), folder.display()));
    }
    // ffmpeg picks one decoder from the first photo, so a second format fails mid-way.
    let formats = source::formats(&images);
    if formats.len() > 1 {
        return Err(format!("mixed image formats ({}) in {}; use a folder with one format", formats.join(", "), folder.display()));
    }
    Ok((folder, images))
}

fn run_convert(
    ffmpeg: Option<&Path>,
    folder: &Path,
    out: &Path,
    settings: &SettingsArgs,
    metadata: &MetadataArgs,
) -> Result<ExitCode, String> {
    let settings = settings.to_settings()?;
    let ffmpeg = binaries::locate(ffmpeg).map_err(|e| e.to_string())?;
    let (folder, images) = scan(folder)?;
    let name = folder.file_name().ok_or("folder has no name")?;
    let metadata = metadata.build(&images[0])?;
    let output = output::reserve(out, name, settings.profile.codec().extension())
        .map_err(|e| format!("{}: {e}", out.display()))?;
    let total = images.len() as u64;
    let result = convert::run(
        &ffmpeg,
        &settings,
        &images,
        &metadata,
        &output,
        |canceller| {
            let handler = move || {
                if let Err(e) = canceller.cancel() {
                    eprintln!("\nerror: could not stop ffmpeg: {e}");
                }
            };
            if let Err(e) = ctrlc::set_handler(handler) {
                eprintln!("warning: Ctrl+C will not clean up: {e}");
            }
        },
        |progress| print_progress(&progress, total),
    );
    eprintln!();
    match result {
        Ok(done) => {
            if let Some(e) = done.key_error {
                eprintln!("warning: video saved without its Apple date and lens keys: {e}");
            }
            println!("{}", output.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(ConvertError::Run(RunError::Cancelled)) => Err("cancelled".to_string()),
        Err(ConvertError::Run(RunError::Failed { code, stderr })) => {
            stderr.iter().skip(stderr.len().saturating_sub(STDERR_TAIL_LINES)).for_each(|line| eprintln!("  {line}"));
            Err(format!("ffmpeg failed (exit code {})", code.map_or("none".to_string(), |c| c.to_string())))
        }
        Err(e) => Err(e.to_string()),
    }
}

fn print_exif(photo: &Path) -> Result<ExitCode, String> {
    let ExifData { make, model, lens, taken_at, offset, gps, artist, copyright } =
        exif::read(photo).map_err(|e| format!("{}: {e}", photo.display()))?;
    let taken = taken_at.map(|t| {
        let mut time = BrokenDownTime::from(t);
        time.set_offset(offset);
        let format = if offset.is_some() { "%Y-%m-%d %H:%M:%S %:z" } else { "%Y-%m-%d %H:%M:%S (no offset)" };
        time.to_string(format).unwrap_or_else(|e| e.to_string())
    });
    let rows = [
        ("make", make),
        ("model", model),
        ("lens", lens),
        ("taken", taken),
        ("location", gps.map(|g| format!("{:.6}, {:.6}", g.lat, g.lon))),
        ("artist", artist),
        ("copyright", copyright),
    ];
    for (label, value) in rows {
        println!("{label:<10} {}", value.as_deref().unwrap_or("-"));
    }
    Ok(ExitCode::SUCCESS)
}

fn geocode(query: &str) -> Result<ExitCode, String> {
    let places = Geocoder::default().search(query).map_err(|e| e.to_string())?;
    if places.is_empty() {
        println!("no places found");
    }
    for place in places {
        println!("{:.6}, {:.6}  {}", place.gps.lat, place.gps.lon, place.name);
    }
    Ok(ExitCode::SUCCESS)
}

/// The app data folder.
fn store() -> Result<Store, String> {
    Store::default_root().map(Store::new).ok_or_else(|| "no home folder for the app data".to_string())
}

fn preset(action: &PresetAction) -> Result<ExitCode, String> {
    let store = store()?;
    match action {
        PresetAction::List => {
            for (id, preset) in store.presets().map_err(|e| e.to_string())? {
                match preset {
                    Ok(p) => println!("{id:<24} {:<24} {}", p.name, describe(&p.settings)),
                    Err(e) => println!("{id:<24} unusable: {e}"),
                }
            }
        }
        PresetAction::Show { id } => {
            let p = store.preset(id).map_err(|e| e.to_string())?;
            println!("{}\n{}", p.name, describe(&p.settings));
        }
        PresetAction::Save { id, name, settings } => {
            // Saved as it will encode, so `show` doesn't list a chroma or range the profile drops.
            let settings = rules::normalize(settings.to_settings()?);
            let preset = Preset { name: name.clone().unwrap_or_else(|| id.clone()), settings };
            store.save_preset(id, &preset).map_err(|e| e.to_string())?;
        }
        PresetAction::Delete { id } => store.delete_preset(id).map_err(|e| e.to_string())?,
    }
    Ok(ExitCode::SUCCESS)
}

fn describe(s: &Settings) -> String {
    let spec = s.profile.spec();
    let size = format!("{}x{}", s.resolution.width(), s.resolution.height());
    format!("{:?} {} {size} {:?} {:?} crf {} {} fps blur {}", spec.codec, spec.id, s.chroma, s.range, s.crf, s.fps, s.blur.frames())
}

/// One line, rewritten in place.
fn print_progress(progress: &Progress, total: u64) {
    let frame = progress.frame.unwrap_or(0);
    let percent = frame * 100 / total;
    let fps = progress.fps.unwrap_or(0.0);
    let eta = if fps > 0.0 {
        let seconds = (total.saturating_sub(frame) as f64 / fps).round() as u64;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    } else {
        "--:--".to_string()
    };
    eprint!("\r{frame}/{total} frames  {percent}%  {fps:.1} fps  ETA {eta}   ");
}
