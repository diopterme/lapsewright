import { ask, message } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { cancelConvert, convert, loadPrefs, openVideo, power, quit, reveal, type Failure, type Power } from "./api";
import { afterQueue, isFinished, move, percent, type Item } from "./queue.ts";
import { t } from "./i18n.ts";

export type NewItem = Pick<Item, "job" | "name" | "extension" | "frames" | "label">;

const ONLY_THIS = t("이것만 취소", "Cancel This One");
const STOP_ALL = t("모두 멈춤", "Stop All");
/** F5 */
const COUNTDOWN_S = 60;

/** The window's list of conversions (Q): one at a time, in order; the backend runs each. */
class Queue {
  items = $state<Item[]>([]);
  running = $derived(this.items.find((i) => i.status === "running") ?? null);
  waiting = $derived(this.items.filter((i) => i.status === "waiting").length);
  /** F5: seconds before the Mac sleeps or shuts down. */
  countdown = $state<{ power: Power; left: number } | null>(null);

  private nextId = 1;
  private isPumping = false;
  private isStoppingAll = false;
  private isAskingQuit = false;
  private timer: ReturnType<typeof setInterval> | undefined;

  add(entry: NewItem) {
    // More work coming: the Mac stays on.
    this.stopCountdown();
    const item: Item = {
      ...entry,
      id: this.nextId++,
      status: "waiting",
      frame: 0,
      fps: null,
      bitrateKbps: null,
      file: null,
      size: null,
      message: null,
      log: [],
      started: null,
      ended: null,
    };
    this.items = [...this.items, item];
    void this.pump();
  }

  /** [다시 넣기]: the same snapshot goes to the back of the line. */
  retry(id: number) {
    const item = this.items.find((i) => i.id === id);
    if (!item) return;
    this.add({ job: item.job, name: item.name, extension: item.extension, frames: item.frames, label: item.label });
  }

  /** Q4: waiting and finished items only; the running one has [취소]. */
  remove(id: number) {
    this.items = this.items.filter((i) => i.id !== id || i.status === "running");
  }

  reorder(from: number, to: number) {
    this.items = move(this.items, from, to);
  }

  /** Q7 */
  clearFinished() {
    this.items = this.items.filter((i) => !isFinished(i.status));
  }

  /** B4, Q6: asks first; the answer may come after this item has finished on its own. */
  async askCancel() {
    const now = this.running;
    if (!now) return;
    const title = t(`${now.name} 변환을 멈출까요?`, `Stop making ${now.name}?`);
    const text = t(`${percent(now)}% 만들었음. 멈추면 만든 부분은 지워짐.`, `${percent(now)}% done. Stopping deletes the part already made.`);
    let stopAll = false;
    if (this.waiting > 0) {
      const buttons = { yes: ONLY_THIS, no: STOP_ALL, cancel: t("계속", "Continue") };
      const answer = await message(t(`${text}\n기다리는 것 ${this.waiting}개`, `${text}\n${this.waiting} waiting`), { title, kind: "warning", buttons });
      if (answer !== ONLY_THIS && answer !== STOP_ALL) return;
      stopAll = answer === STOP_ALL;
    } else if (!(await ask(text, { title, kind: "warning", okLabel: t("멈춤", "Stop"), cancelLabel: t("계속", "Continue") }))) {
      return;
    }
    if (this.running?.id !== now.id) return;
    this.isStoppingAll = stopAll;
    await cancelConvert();
  }

  /** V5, Q8: window close or ⌘Q while converting; the queue is not kept. */
  async askQuit() {
    if (this.isAskingQuit) return;
    this.isAskingQuit = true;
    try {
      const done = this.running ? percent(this.running) : 0;
      const lost = this.waiting > 0 ? t(`\n기다리는 ${this.waiting}개도 사라짐.`, `\nThe ${this.waiting} waiting will be dropped too.`) : "";
      const yes = await ask(t(`끄면 변환을 멈추고 만든 부분은 지움.${lost}`, `Quitting stops the conversion and deletes the part already made.${lost}`), {
        title: t(`변환 중 (${done}%)`, `Converting (${done}%)`),
        kind: "warning",
        okLabel: t("멈추고 끄기", "Stop and Quit"),
        cancelLabel: t("계속", "Continue"),
      });
      if (yes) await quit();
    } finally {
      this.isAskingQuit = false;
    }
  }

  private update(id: number, patch: Partial<Item>) {
    this.items = this.items.map((i) => (i.id === id ? { ...i, ...patch } : i));
  }

  private async pump() {
    if (this.isPumping) return;
    this.isPumping = true;
    try {
      const batch: number[] = [];
      for (let next = this.firstWaiting(); next; next = this.firstWaiting()) {
        await this.run(next);
        batch.push(next.id);
      }
      if (batch.length) void this.afterAll(batch);
    } finally {
      this.isPumping = false;
    }
  }

  /** [취소] on the countdown. */
  stopCountdown() {
    clearInterval(this.timer);
    this.countdown = null;
  }

  /** [지금 …], or the countdown reached 0. */
  async powerNow() {
    const now = this.countdown;
    this.stopCountdown();
    if (!now) return;
    try {
      await power(now.power);
    } catch (e) {
      await message(String(e), { title: now.power === "sleep" ? t("잠자기 못 함", "Couldn’t Sleep") : t("시스템 종료 못 함", "Couldn’t Shut Down"), kind: "warning" });
    }
  }

  /** F4: the settings window's choices, read now so a change made while converting counts. */
  private async afterAll(ids: number[]) {
    const batch = ids.map((id) => this.items.find((i) => i.id === id)).filter((i) => i !== undefined);
    const prefs = await loadPrefs();
    const { video, power } = afterQueue(batch, prefs);
    const cannotOpen = (e: unknown) => message(String(e), { title: t("영상을 못 엶", "Couldn’t Open the Video"), kind: "warning" });
    if (video && prefs.reveal) await reveal(video).catch(cannotOpen);
    if (video && prefs.open) await openVideo(video).catch(cannotOpen);
    if (power === "none" || this.firstWaiting()) return;
    this.countdown = { power, left: COUNTDOWN_S };
    // Only the countdown is out of sight; it still runs.
    getCurrentWindow().setFocus().catch(() => {});
    this.timer = setInterval(() => {
      if (!this.countdown) return;
      if (this.countdown.left > 1) this.countdown = { ...this.countdown, left: this.countdown.left - 1 };
      else void this.powerNow();
    }, 1000);
  }

  private firstWaiting = () => this.items.find((i) => i.status === "waiting");

  private async run(item: Item) {
    this.update(item.id, { status: "running", started: Date.now() });
    let command: string[] = [];
    try {
      const done = await convert(item.job, (e) => {
        if (e.kind === "started") {
          command = [`$ ${e.command}`];
          this.update(item.id, { file: e.file, frames: e.frames });
        } else {
          this.update(item.id, { frame: e.frame, fps: e.fps, bitrateKbps: e.bitrateKbps });
        }
      });
      const status = done.keyError ? "warn" : "done";
      this.update(item.id, { status, file: done.file, size: done.size, message: done.keyError, log: [...command, ...done.log] });
    } catch (e) {
      const failure = asFailure(e);
      const status = failure.cancelled ? "cancelled" : "failed";
      this.update(item.id, { status, message: failure.message, log: [...command, ...failure.log] });
    }
    this.update(item.id, { ended: Date.now() });
    if (this.isStoppingAll) {
      this.isStoppingAll = false;
      this.items = this.items.map((i) => (i.status === "waiting" ? { ...i, status: "cancelled", message: t("취소됨", "Cancelled") } : i));
    }
  }
}

/** The backend rejects with a `Failure`; anything else is the IPC itself failing. */
function asFailure(e: unknown): Failure {
  if (typeof e === "object" && e !== null && "message" in e && "cancelled" in e) return e as Failure;
  return { cancelled: false, message: String(e), log: [] };
}

export const queue = new Queue();
