import type { Chroma, Codec, Profile, Range, Settings } from "./api";
import { fpsValue } from "./format.ts";
import { t } from "./i18n.ts";

/** D5: QuickTime's names for the codecs. */
export const USES: { codec: Codec; label: string; title: string }[] = [
  { codec: "h264", label: "H.264", title: t("호환성 — 어디서나 재생", "Compatible — plays anywhere") },
  { codec: "hevc", label: "H.265", title: t("압축 — 같은 화질에 파일 작음 (HEVC)", "Compact — smaller file, same quality (HEVC)") },
  { codec: "prores", label: "ProRes", title: t("화질 — 편집용, 파일 큼 (Apple ProRes)", "Quality — for editing, large file (Apple ProRes)") },
];

/** C4 */
export const SIZES: [number, number][] = [
  [7680, 4320],
  [4096, 2304],
  [3840, 2160],
  [2560, 1440],
  [1920, 1080],
  [1280, 720],
];
/** core settings.rs `MAX_SIDE` */
export const MAX_SIDE = 8192;

/** C5, as core `Fps` writes them. */
export const FPS_LIST = ["24000/1001", "24", "25", "30000/1001", "30", "50", "60000/1001", "60"];
/** C6 */
export const BLURS = [0, 5, 10, 15, 20];
export const CHROMAS: Chroma[] = ["yuv420", "yuv422", "yuv444"];
export const RANGES: Range[] = ["sdr", "hlg", "hdr"];
export const RANGE_TEXT: Record<Range, string> = { sdr: "SDR", hlg: "HLG", hdr: "HDR10" };

/** C7: named points on the quality slider; a lower crf is better. */
export const CRF_STEPS = [
  { label: t("작게", "Small"), crf: 28 },
  { label: t("낮음", "Low"), crf: 23 },
  { label: t("보통", "Mid"), crf: 18 },
  { label: t("높음", "High"), crf: 13 },
  { label: t("최고", "Best"), crf: 8 },
];
/** core settings.rs `MAX_CRF` */
export const MAX_CRF = 51;

/** "yuv420" → "4:2:0" */
export const chromaText = (c: Chroma) => `${c[3]}:${c[4]}:${c[5]}`;

/** C2: `high10  10비트 · 4:2:0 · SDR/HDR` */
export function profileLine(p: Profile): string {
  const [first, last] = [p.chroma[0], p.chroma[p.chroma.length - 1]];
  const chroma = first === last ? chromaText(first) : `${chromaText(first)}–${chromaText(last)}`;
  return `${p.id} ${p.bits}${t("비트", "-bit")} · ${chroma} · ${p.hdr ? "SDR/HDR" : "SDR"}`;
}

export const findProfile = (list: Profile[], s: Settings) => list.find((p) => p.codec === s.codec && p.id === s.profile);

/** D5: one line under a row with greyed-out choices. */
export const chromaWhy = (p: Profile) =>
  p.chroma.length < CHROMAS.length ? t(`이 프로파일은 ${p.chroma.map(chromaText).join(" · ")} 만`, `This profile: ${p.chroma.map(chromaText).join(" · ")} only`) : null;

export function hdrWhy(list: Profile[], p: Profile): string | null {
  if (p.hdr) return null;
  const ids = list.filter((x) => x.codec === p.codec && x.hdr).map((x) => x.id);
  return t(`HDR 은 ${ids.join(" · ")} 에서`, `HDR needs ${ids.join(" · ")}`);
}

/** C3: the use's default profile; the backend then replaces what that profile can't do. */
export function switchUse(s: Settings, codec: Codec, list: Profile[]): Settings {
  const profile = list.find((p) => p.codec === codec && p.isDefault);
  return profile ? { ...s, codec, profile: profile.id } : s;
}

/** C3: values the backend replaced, as `HDR10 → SDR, 4:2:2 → 4:2:0`. */
export function replaced(asked: Settings, got: Settings): { fields: (keyof Settings)[]; text: string } {
  const shown: [keyof Settings, (s: Settings) => string][] = [
    ["range", (s) => RANGE_TEXT[s.range]],
    ["chroma", (s) => chromaText(s.chroma)],
    ["crf", (s) => t(`화질 ${s.crf}`, `Quality ${s.crf}`)],
  ];
  const changed = shown.filter(([key]) => asked[key] !== got[key]);
  return { fields: changed.map(([key]) => key), text: changed.map(([, text]) => `${text(asked)} → ${text(got)}`).join(", ") };
}

export const sameSettings = (a: Settings, b: Settings) => (Object.keys(a) as (keyof Settings)[]).every((k) => a[k] === b[k]);

/** D5: the folded 고급 line, `high · 4:2:0 · SDR · 화질 8`. ProRes has no crf. */
export function advancedSummary(s: Settings): string {
  const parts = [s.profile, chromaText(s.chroma), RANGE_TEXT[s.range]];
  return (s.codec === "prores" ? parts : [...parts, t(`화질 ${s.crf}`, `Quality ${s.crf}`)]).join(" · ");
}

/** core settings.rs `Resolution::new`: even, 2 to `MAX_SIDE`. */
export const isValidSide = (n: number) => Number.isInteger(n) && n >= 2 && n <= MAX_SIDE && n % 2 === 0;

/** Apple ProRes white paper, 1920×1080 at 29.97 fps, in Mbit/s; scales with pixels and fps. */
const PRORES_MBPS: Record<string, number> = { proxy: 45, lt: 102, standard: 147, hq: 220, "4444": 330, "4444xq": 500 };
/** Bits per pixel at crf 18. h264: one measured timelapse (4K crf 8, 202 Mbit/s). ponytail: hevc
 *  is half of that, not measured; photos vary 2–3× (B3), so the start line says "약". */
const BITS_PER_PIXEL: Record<string, number> = { h264: 0.22, hevc: 0.11 };
/** x264 and x265 roughly double the size every 6 crf steps down. */
const CRF_DOUBLING = 6;

/** B3: bytes of the finished video, before any frame is made. */
export function estimateBytes(s: Settings, frames: number): number {
  const fps = fpsValue(s.fps);
  const pixelsPerSecond = s.width * s.height * fps;
  const bitsPerSecond =
    s.codec === "prores"
      ? (PRORES_MBPS[s.profile] ?? PRORES_MBPS.hq) * 1e6 * (pixelsPerSecond / (1920 * 1080 * 30000 / 1001))
      : pixelsPerSecond * BITS_PER_PIXEL[s.codec] * 2 ** ((18 - s.crf) / CRF_DOUBLING);
  return (bitsPerSecond / 8) * (frames / fps);
}
