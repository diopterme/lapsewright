import { Channel, invoke } from "@tauri-apps/api/core";

/** crates/app/src/folder.rs `Folder` */
export type Folder = {
  path: string;
  /** Where the video goes unless another folder was picked. */
  parent: string | null;
  name: string;
  names: string[];
  formats: string[];
  firstTaken: string | null;
  lastTaken: string | null;
  /** Core `Settings::crop` last dragged in this folder. */
  crop: number;
};

export type Recent = { path: string; name: string; count: number; exists: boolean };

export type Codec = "h264" | "hevc" | "prores";
export type Chroma = "yuv420" | "yuv422" | "yuv444";
export type Range = "sdr" | "hlg" | "hdr";

/** core store.rs `SettingsFile` */
export type Settings = {
  codec: Codec;
  profile: string;
  chroma: Chroma;
  range: Range;
  width: number;
  height: number;
  crf: number;
  fps: string;
  blur: number;
};

/** crates/app/src/presets.rs `ProfileInfo` */
export type Profile = {
  codec: Codec;
  id: string;
  bits: number;
  chroma: Chroma[];
  hdr: boolean;
  isDefault: boolean;
  extension: string;
};

/** crates/app/src/session.rs `Session`; `output: null` = next to the photo folder. */
export type Session = { preset: string | null; settings: Settings; output: string | null };

export type PresetEntry = { id: string; name: string; settings: Settings | null; error: string | null };

export const openFolder = (path: string) => invoke<Folder>("open_folder", { path });
/** After a drag of the crop frame. */
export const setCrop = (path: string, crop: number) => invoke<void>("set_crop", { path, crop });
export const recentFolders = () => invoke<Recent[]>("recent_folders");
/** Y10: object URL of the folder's first photo; the caller revokes it. */
export async function recentThumbUrl(path: string): Promise<string> {
  const bytes = await invoke<ArrayBuffer>("recent_thumb", { path });
  return URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" }));
}
export const photoTaken = (index: number) => invoke<string | null>("photo_taken", { index });

export const loadSession = () => invoke<Session>("session");
export const setSession = (session: Session) => invoke<Session>("set_session", { session });
export const outputProblem = (dir: string) => invoke<string | null>("output_problem", { dir });

export const profiles = () => invoke<Profile[]>("profiles");
export const presets = () => invoke<PresetEntry[]>("presets");
export const savePreset = (id: string | null, name: string, settings: Settings) =>
  invoke<string>("save_preset", { id, name, settings });
export const deletePreset = (id: string) => invoke<void>("delete_preset", { id });
export const restoreBuiltIns = (overwrite: boolean) => invoke<string[]>("restore_built_ins", { overwrite });

/** Object URL of the JPEG; the caller revokes it. */
export async function previewUrl(index: number, original: boolean): Promise<string> {
  const bytes = await invoke<ArrayBuffer>("preview", { index, original });
  return URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" }));
}

/** crates/app/src/convert.rs: what [시작] takes from the window. */
export type Job = { folder: string; settings: Settings; output: string; crop: number };

export type ConvertEvent =
  | { kind: "started"; file: string; command: string; frames: number }
  | { kind: "progress"; frame: number; fps: number | null; bitrateKbps: number | null };

export type Done = { file: string; size: number; keyError: string | null; log: string[] };
/** What `convert` rejects with. */
export type Failure = { cancelled: boolean; message: string; log: string[] };

/** B1–B3: `file` is the name [시작] would get now (`_2` when taken). */
export type Target = { problem: string | null; file: string | null; free: number | null };

export function convert(job: Job, onEvent: (e: ConvertEvent) => void): Promise<Done> {
  const channel = new Channel<ConvertEvent>();
  channel.onmessage = onEvent;
  return invoke<Done>("convert", { job, onEvent: channel });
}
export const cancelConvert = () => invoke<void>("cancel_convert");
/** Stops the conversion, deletes the partial video, quits. */
export const quit = () => invoke<void>("quit");
export const ffmpegProblem = () => invoke<string | null>("ffmpeg_problem");
export const target = (dir: string, name: string, extension: string) => invoke<Target>("target", { dir, name, extension });
export const reveal = (path: string) => invoke<void>("reveal", { path });
export const openVideo = (path: string) => invoke<void>("open_video", { path });

/** crates/app/src/prefs.rs `Prefs`: the settings window. Empty author or copyright = the photo's EXIF. */
export type Power = "none" | "sleep" | "shutdown";
export type Prefs = { author: string; copyright: string; reveal: boolean; open: boolean; power: Power };

export const loadPrefs = () => invoke<Prefs>("prefs");
export const setPrefs = (prefs: Prefs) => invoke<void>("set_prefs", { prefs });
/** `Kim` → `Copyright 2026 Kim`, as the video gets it. */
export const copyrightText = (holder: string) => invoke<string>("copyright_text", { holder });
export const clearRecent = () => invoke<void>("clear_recent");
export const openSettings = () => invoke<void>("open_settings");
export const power = (power: Power) => invoke<void>("power", { power });
/** Settings › About; the backend opens only our GitHub and ffmpeg.org pages. */
export const openLink = (url: string) => invoke<void>("open_link", { url });
