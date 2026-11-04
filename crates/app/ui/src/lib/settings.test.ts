// node --test src/lib/
import { test } from "node:test";
import assert from "node:assert/strict";
import type { Profile, Settings } from "./api.ts";
import { advancedSummary, chromaWhy, estimateBytes, hdrWhy, isValidSide, profileLine, replaced, sameSettings, switchUse } from "./settings.ts";

const profile = (codec: Profile["codec"], id: string, bits: number, chroma: Profile["chroma"], hdr: boolean, isDefault = false): Profile =>
  ({ codec, id, bits, chroma, hdr, isDefault, extension: codec === "h264" ? "mp4" : "mov" });

const LIST = [
  profile("h264", "high", 8, ["yuv420"], false, true),
  profile("h264", "high10", 10, ["yuv420"], true),
  profile("h264", "high422", 10, ["yuv420", "yuv422"], true),
  profile("prores", "lt", 10, ["yuv422"], false),
  profile("prores", "hq", 10, ["yuv422"], true, true),
];

const HQ: Settings = { codec: "prores", profile: "hq", chroma: "yuv422", range: "hdr", width: 4096, height: 2304, crf: 8, fps: "30000/1001", blur: 5 };

test("profile menu line says bits, chroma, SDR/HDR in that order", () => {
  assert.equal(profileLine(LIST[1]), "high10 10비트 · 4:2:0 · SDR/HDR");
  assert.equal(profileLine(LIST[2]), "high422 10비트 · 4:2:0–4:2:2 · SDR/HDR");
  assert.equal(profileLine(LIST[3]), "lt 10비트 · 4:2:2 · SDR");
});

test("switching use picks that codec's default profile and keeps the rest", () => {
  assert.deepEqual(switchUse(HQ, "h264", LIST), { ...HQ, codec: "h264", profile: "high" });
});

test("replaced values are named, as the panel's one-line notice", () => {
  const asked = { ...HQ, codec: "h264" as const, profile: "high" };
  const got = { ...asked, chroma: "yuv420" as const, range: "sdr" as const };
  assert.deepEqual(replaced(asked, got), { fields: ["range", "chroma"], text: "HDR10 → SDR, 4:2:2 → 4:2:0" });
  assert.deepEqual(replaced(got, got), { fields: [], text: "" });
});

test("greyed-out choices say why", () => {
  assert.equal(chromaWhy(LIST[2]), "이 프로파일은 4:2:0 · 4:2:2 만");
  assert.equal(hdrWhy(LIST, LIST[0]), "HDR 은 high10 · high422 에서");
  assert.equal(hdrWhy(LIST, LIST[1]), null);
});

test("folded line leaves out crf for ProRes", () => {
  assert.equal(advancedSummary(HQ), "hq · 4:2:2 · HDR10");
  assert.equal(advancedSummary({ ...HQ, codec: "h264", profile: "high", chroma: "yuv420", range: "sdr" }), "high · 4:2:0 · SDR · 화질 8");
});

test("sizes must be even and at most 8192", () => {
  assert.deepEqual([2, 8192, 1, 1921, 8194, 0, 2.5].map(isValidSide), [true, true, false, false, false, false, false]);
});

test("changed settings are not the same", () => {
  assert.ok(sameSettings(HQ, { ...HQ }));
  assert.ok(!sameSettings(HQ, { ...HQ, blur: 0 }));
});

test("ProRes size follows the white paper rate; crf 12 is about twice crf 18", () => {
  const prores: Settings = { codec: "prores", profile: "hq", chroma: "yuv422", range: "sdr", width: 1920, height: 1080, crf: 18, fps: "30000/1001", blur: 0 };
  // 220 Mbit/s for 100 s.
  assert.equal(Math.round(estimateBytes(prores, 2997) / 1e6), 2750);
  const h264: Settings = { ...prores, codec: "h264", profile: "high", chroma: "yuv420" };
  assert.equal(Math.round(estimateBytes({ ...h264, crf: 12 }, 2997) / estimateBytes(h264, 2997)), 2);
});
