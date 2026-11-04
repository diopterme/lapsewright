// node --test src/lib/
import { test } from "node:test";
import assert from "node:assert/strict";
import { afterQueue, move, percent, remaining, type Item, type Status } from "./queue.ts";

const item = (frame: number, frames: number, fps: number | null) => ({ frame, frames, fps }) as Item;

test("moving keeps every item once", () => {
  assert.deepEqual(move(["a", "b", "c", "d"], 0, 2), ["b", "c", "a", "d"]);
  assert.deepEqual(move(["a", "b", "c", "d"], 3, 0), ["d", "a", "b", "c"]);
  assert.deepEqual(move(["a", "b", "c"], 1, 9), ["a", "c", "b"]);
  const same = ["a", "b"];
  assert.equal(move(same, 1, 1), same);
});

test("progress is whole percent and time left follows the speed", () => {
  assert.equal(percent(item(1140, 3000, 28.4)), 38);
  assert.equal(percent(item(3001, 3000, null)), 100);
  assert.equal(percent(item(0, 0, null)), 0);
  assert.equal(remaining(item(1000, 3000, 20)), 100);
  assert.equal(remaining(item(0, 3000, null)), null);
});

const ran = (status: Status, file: string | null) => ({ status, file }) as Item;

test("after the queue: the last video, and power only when nothing failed", () => {
  const sleep = { power: "sleep" } as const;
  assert.deepEqual(afterQueue([ran("done", "/a.mp4"), ran("warn", "/b.mp4")], sleep), { video: "/b.mp4", power: "sleep" });
  assert.deepEqual(afterQueue([ran("done", "/a.mp4"), ran("failed", null)], sleep), { video: "/a.mp4", power: "none" });
  assert.deepEqual(afterQueue([ran("cancelled", null)], sleep), { video: null, power: "none" });
  assert.deepEqual(afterQueue([], sleep), { video: null, power: "none" });
});
