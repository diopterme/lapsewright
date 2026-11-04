import { t } from "./i18n.ts";

const number = new Intl.NumberFormat(t("ko-KR", "en-US"));

export const count = (n: number) => number.format(n);

/** "30000/1001" → frames per second */
export function fpsValue(fps: string): number {
  const [num, den = "1"] = fps.split("/");
  return Number(num) / Number(den);
}

/** 29.97, 25 */
export const fpsText = (fps: string) => String(Math.round(fpsValue(fps) * 100) / 100);

/** 100 → "1:40", 3725 → "1:02:05" (rounded down, like a player's clock) */
export function clock(seconds: number): string {
  const s = Math.floor(seconds);
  const [h, m, sec] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60];
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

/** "2026-10-20T05:12:00" → "2026-10-20 05:12:00" */
export const takenText = (iso: string) => iso.replace("T", " ");

/** "10/20 05:12 ~ 09:22", or with both dates when the shoot crosses midnight. */
export function period(first: string | null, last: string | null): string | null {
  if (!first || !last) return null;
  const short = (iso: string) => ({ date: `${+iso.slice(5, 7)}/${+iso.slice(8, 10)}`, time: iso.slice(11, 16) });
  const [a, b] = [short(first), short(last)];
  return a.date === b.date ? `${a.date} ${a.time} ~ ${b.time}` : `${a.date} ${a.time} ~ ${b.date} ${b.time}`;
}

export type Box = { x: number; y: number; w: number; h: number };

/** Core `CROP_END`: the crop position goes from 0 to this, half is the middle. */
export const CROP_END = 1000;

/** The part of a photo the video keeps, as fractions: the builder's crop to the video's aspect,
 *  `at` 0–1 along the side with room. `null` when the aspects match (the preview rounds sizes,
 *  hence the tolerance). */
export function cropBox(photoW: number, photoH: number, videoW: number, videoH: number, at: number): Box | null {
  const w = Math.min(1, (photoH * videoW) / videoH / photoW);
  const h = Math.min(1, (photoW * videoH) / videoW / photoH);
  if (w > 0.995 && h > 0.995) return null;
  return { x: (1 - w) * at, y: (1 - h) * at, w, h };
}

/** `at` after dragging `box` by fractions of the photo; only the side with room counts. */
export function dragCrop(box: Box, dx: number, dy: number): number {
  const [from, room] = box.w < 1 ? [box.x + dx, 1 - box.w] : [box.y + dy, 1 - box.h];
  return Math.min(1, Math.max(0, from / room));
}

/** Largest size of the given aspect that fits the area. */
export function fit(w: number, h: number, areaW: number, areaH: number): { w: number; h: number } {
  const scale = Math.min(areaW / w, areaH / h);
  return { w: Math.floor(w * scale), h: Math.floor(h * scale) };
}

/** Decimal units, as Finder shows sizes: 1.2GB, 820MB. */
export function bytesText(n: number): string {
  if (n >= 1e11) return `${Math.round(n / 1e9)}GB`;
  if (n >= 1e9) return `${Math.round(n / 1e8) / 10}GB`;
  if (n >= 1e6) return `${Math.round(n / 1e6)}MB`;
  return `${Math.max(1, Math.round(n / 1e3))}KB`;
}

/** 252 → "4분 12초", 3720 → "1시간 2분" */
export function spanText(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const [h, m, sec] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60];
  if (h > 0) return m > 0 ? t(`${h}시간 ${m}분`, `${h} h ${m} min`) : t(`${h}시간`, `${h} h`);
  if (m > 0) return sec > 0 ? t(`${m}분 ${sec}초`, `${m} min ${sec} s`) : t(`${m}분`, `${m} min`);
  return t(`${sec}초`, `${sec} s`);
}
