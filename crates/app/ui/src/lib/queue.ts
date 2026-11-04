import type { Job, Power, Prefs } from "./api";

/** Q5–Q7: finished, failed and cancelled items stay until [끝난 것 지우기]. */
export type Status = "waiting" | "running" | "done" | "warn" | "failed" | "cancelled";

export type Item = {
  id: number;
  job: Job;
  /** The folder name; the video gets it, plus `_2` when taken (B2). */
  name: string;
  extension: string;
  frames: number;
  /** `4096×2304 · 29.97 fps · H.264` */
  label: string;
  status: Status;
  frame: number;
  fps: number | null;
  bitrateKbps: number | null;
  /** Picked by the backend when the item starts. */
  file: string | null;
  size: number | null;
  /** E3 for failed, the key error for warn. */
  message: string | null;
  /** E4: the ffmpeg command, then everything it wrote. */
  log: string[];
  started: number | null;
  ended: number | null;
};

export const isFinished = (s: Status) => s === "done" || s === "warn" || s === "failed" || s === "cancelled";

export const percent = (item: Item) => (item.frames > 0 ? Math.min(100, Math.floor((item.frame * 100) / item.frames)) : 0);

/** Seconds left at the current encoding speed; `null` until ffmpeg reports one. */
export const remaining = (item: Item) => (item.fps ? (item.frames - item.frame) / item.fps : null);

/** B3: the size the video heads for at the bitrate so far. */
export const liveBytes = (item: Item, videoFps: number) =>
  item.bitrateKbps ? (item.bitrateKbps * 1000 / 8) * (item.frames / videoFps) : null;

/** Q4: `list` with the item at `from` moved to `to`. */
export function move<T>(list: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || from >= list.length) return list;
  const rest = list.filter((_, i) => i !== from);
  const at = Math.max(0, Math.min(to, rest.length));
  return [...rest.slice(0, at), list[from], ...rest.slice(at)];
}

/** F4, F5: once the queue is done. `batch` is what ran since it last was; the last video gets
 *  shown, and the Mac sleeps or shuts down only if every item made one. */
export function afterQueue(batch: Item[], prefs: Pick<Prefs, "power">): { video: string | null; power: Power } {
  const made = batch.filter((i) => (i.status === "done" || i.status === "warn") && i.file);
  const isClean = batch.length > 0 && made.length === batch.length;
  return { video: made.at(-1)?.file ?? null, power: isClean ? prefs.power : "none" };
}
