// node --test src/lib/
import { test } from "node:test";
import assert from "node:assert/strict";
import { bytesText, clock, cropBox, dragCrop, fit, fpsText, fpsValue, period, spanText } from "./format.ts";

test("video time of 3,000 photos at 29.97 fps is 1:40", () => {
  assert.equal(clock(3000 / fpsValue("30000/1001")), "1:40");
  assert.equal(clock(3725.9), "1:02:05");
  assert.equal(fpsText("30000/1001"), "29.97");
  assert.equal(fpsText("25"), "25");
});

test("3:2 photo in a 16:9 video loses top and bottom", () => {
  const box = cropBox(1280, 854, 4096, 2304, 0.5)!;
  assert.equal(box.w, 1);
  assert.ok(Math.abs(box.h - 0.8433) < 0.001, String(box.h));
  assert.ok(Math.abs(box.y - (1 - box.h) / 2) < 1e-9);
});

test("crop position moves the frame from the top edge to the bottom edge", () => {
  assert.equal(cropBox(1280, 854, 4096, 2304, 0)!.y, 0);
  const bottom = cropBox(1280, 854, 4096, 2304, 1)!;
  assert.ok(Math.abs(bottom.y + bottom.h - 1) < 1e-9);
  assert.equal(cropBox(1280, 854, 4096, 2304, 1)!.x, 0);
});

test("dragging moves only the side with room, and stops at the edges", () => {
  const wide = cropBox(1280, 854, 4096, 2304, 0.5)!;
  assert.equal(dragCrop(wide, 0.3, 0), 0.5);
  assert.ok(Math.abs(dragCrop(wide, 0, -(1 - wide.h) / 4) - 0.25) < 1e-9);
  assert.equal(dragCrop(wide, 0, 5), 1);
  assert.equal(dragCrop(wide, 0, -5), 0);
  const tall = cropBox(4096, 1000, 1920, 1080, 0.5)!;
  assert.ok(tall.w < 1 && Math.abs(dragCrop(tall, (1 - tall.w) / 2, 0.3) - 1) < 1e-9);
});

test("portrait photo in a 16:9 video loses top and bottom more", () => {
  const box = cropBox(854, 1280, 4096, 2304, 0.5)!;
  assert.equal(box.w, 1);
  assert.ok(box.h < 0.4);
});

test("same aspect shows no crop, despite rounding", () => {
  assert.equal(cropBox(1280, 720, 4096, 2304, 0.5), null);
  assert.equal(cropBox(1280, 721, 3840, 2160, 0.5), null);
});

test("period shows the date once when the shoot stays in one day", () => {
  assert.equal(period("2026-10-20T05:12:00", "2026-10-20T09:22:59"), "10/20 05:12 ~ 09:22");
  assert.equal(period("2026-10-20T22:00:00", "2026-10-21T02:30:00"), "10/20 22:00 ~ 10/21 02:30");
  assert.equal(period(null, "2026-10-20T09:22:59"), null);
});

test("fit keeps the aspect inside the area", () => {
  assert.deepEqual(fit(1280, 854, 800, 800), { w: 800, h: 533 });
  assert.deepEqual(fit(854, 1280, 800, 400), { w: 266, h: 400 });
});

test("sizes read like Finder", () => {
  assert.equal(bytesText(1_234_000_000), "1.2GB");
  assert.equal(bytesText(182_400_000_000), "182GB");
  assert.equal(bytesText(820_400_000), "820MB");
  assert.equal(bytesText(12_000), "12KB");
  assert.equal(bytesText(0), "1KB");
});

test("spans read as hours, minutes and seconds", () => {
  assert.equal(spanText(252), "4분 12초");
  assert.equal(spanText(120), "2분");
  assert.equal(spanText(3720), "1시간 2분");
  assert.equal(spanText(7.4), "7초");
});
