<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    ffmpegProblem,
    loadPrefs,
    loadSession,
    openFolder,
    setCrop,
    profiles as loadProfiles,
    recentFolders,
    setSession,
    target as loadTarget,
    type Folder,
    type Prefs,
    type Profile,
    type Recent,
    type Session,
    type Target,
  } from "./lib/api";
  import { bytesText, count, fpsText, fpsValue, spanText } from "./lib/format";
  import { USES, estimateBytes } from "./lib/settings";
  import { isFinished } from "./lib/queue.ts";
  import { queue } from "./lib/queue.svelte.ts";
  import FolderBar from "./lib/FolderBar.svelte";
  import EmptyView from "./lib/EmptyView.svelte";
  import Preview from "./lib/Preview.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import QueuePanel from "./lib/QueuePanel.svelte";
  import PowerCountdown from "./lib/PowerCountdown.svelte";
  import { t } from "./lib/i18n";

  /** L3: settings panel width, dragged by the edge and remembered. */
  const SIDE_MIN = 260;
  const SIDE_MAX = 420;
  const SIDE_DEFAULT = 300;
  const SIDE_KEY = "lapsewright.sideWidth";

  let folder = $state<Folder | null>(null);
  /** The folder's crop position; [시작] takes it as it is then. */
  let cropAt = $state(0);
  let session = $state<Session | null>(null);
  let profiles = $state<Profile[]>([]);
  let recent = $state<Recent[]>([]);
  let error = $state<string | null>(null);
  let dragOver = $state(false);
  let side = $state(readSide());
  /** Q1 */
  let tab = $state<"settings" | "queue">("settings");
  let openLog = $state<number | null>(null);
  let ffmpegIssue = $state<string | null>(null);
  let target = $state<Target | null>(null);
  /** M3: author and copyright under the output folder. */
  let prefs = $state<Prefs | null>(null);

  function readSide(): number {
    try {
      const saved = Number(localStorage.getItem(SIDE_KEY));
      return saved >= SIDE_MIN && saved <= SIDE_MAX ? saved : SIDE_DEFAULT;
    } catch {
      return SIDE_DEFAULT;
    }
  }

  async function load(path: string) {
    try {
      folder = await openFolder(path);
      cropAt = folder.crop;
      error = null;
    } catch (e) {
      // Back to the start view: it is the only place the reason is shown.
      folder = null;
      error = String(e);
    }
    recent = await recentFolders().catch(() => recent);
  }

  async function pick() {
    const path = await open({ directory: true, title: t("사진 폴더 열기", "Open Photo Folder") }).catch((e) => {
      error = String(e);
      return null;
    });
    if (typeof path === "string") load(path);
  }

  onMount(() => {
    Promise.all([loadSession(), loadProfiles()]).then(
      ([s, p]) => [session, profiles] = [s, p],
      (e) => (error = String(e)),
    );
    recentFolders().then((r) => (recent = r), () => {});
    ffmpegProblem().then((p) => (ffmpegIssue = p), (e) => (ffmpegIssue = String(e)));
    loadPrefs().then((p) => (prefs = p), (e) => (error = String(e)));
    const stopQuit = listen("quit-requested", () => queue.askQuit());
    const stopPrefs = listen<Prefs>("prefs-changed", (e) => (prefs = e.payload));
    const stopRecent = listen("recent-cleared", () => (recent = []));
    // L7: a folder or a photo dropped anywhere on the window.
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragOver = payload.type === "enter" || payload.type === "over";
      if (payload.type === "drop" && payload.paths[0]) load(payload.paths[0]);
    });
    return () => {
      void unlisten.then((stop) => stop());
      for (const stop of [stopQuit, stopPrefs, stopRecent]) void stop.then((s) => s());
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (!e.metaKey) return;
    // B4: ⌘↩ start, ⌘. cancel (asks first).
    const actions: Record<string, () => void> = { o: pick, enter: start, ".": () => queue.askCancel() };
    const action = actions[e.key.toLowerCase()];
    if (!action) return;
    e.preventDefault();
    action();
  }

  /** Every panel change goes through the backend, which previews use and which checks the values. */
  async function apply(next: Session): Promise<Session> {
    const got = await setSession(next);
    session = got;
    return got;
  }

  function resize(e: PointerEvent) {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const onmove = (m: PointerEvent) => {
      side = Math.min(Math.max(window.innerWidth - m.clientX, SIDE_MIN), SIDE_MAX);
    };
    const onup = () => {
      handle.removeEventListener("pointermove", onmove);
      try {
        localStorage.setItem(SIDE_KEY, String(side));
      } catch {
        // Only the width is forgotten.
      }
    };
    handle.addEventListener("pointermove", onmove);
    handle.addEventListener("pointerup", onup, { once: true });
  }

  const settings = $derived(session?.settings);
  const extension = $derived(profiles.find((p) => p.codec === settings?.codec)?.extension);
  /** C10: next to the photo folder unless another was picked. */
  const outputDir = $derived(session?.output ?? folder?.parent ?? null);
  /** A finished conversion may have taken the name the start line shows. */
  const finishedCount = $derived(queue.items.filter((i) => isFinished(i.status)).length);

  $effect(() => {
    const [dir, name, ext] = [outputDir, folder?.name, extension];
    void finishedCount;
    void queue.running?.file;
    target = null;
    if (!dir || !name || !ext) return;
    loadTarget(dir, name, ext).then(
      (t) => dir === outputDir && name === folder?.name && ext === extension && (target = t),
      (e) => (target = { problem: String(e), file: null, free: null }),
    );
  });

  const settingsLine = $derived(
    settings ? `${settings.width}×${settings.height} · ${fpsText(settings.fps)} fps · ${USES.find((u) => u.codec === settings.codec)?.label}` : "",
  );

  // D3 + B2 + B3, Y1: what [시작] will make, under the panel.
  const summary = $derived.by(() => {
    if (!folder || !settings) return null;
    const frames = folder.names.length;
    const size = bytesText(estimateBytes(settings, frames));
    return {
      length: t(`${spanText(frames / fpsValue(settings.fps))} 영상`, `${spanText(frames / fpsValue(settings.fps))} video`),
      spec: `${settingsLine} · ${settings.codec === "prores" ? size : t(`약 ${size}`, `about ${size}`)}`,
      file: [`→ ${target?.file ?? `${folder.name}.${extension}`}`, t(`${count(frames)}장`, `${count(frames)} photos`), target?.free != null ? t(`여유 ${bytesText(target.free)}`, `${bytesText(target.free)} free`) : null]
        .filter((p) => p !== null)
        .join(" · "),
    };
  });

  // B1: why [시작] can't be pressed.
  const reason = $derived(ffmpegIssue ?? (target?.problem ? t(`출력 폴더: ${target.problem} — 저장 상자의 [변경…]`, `Output folder: ${target.problem}. Use [Change…] under Save.`) : null));

  function moveCrop(at: number) {
    cropAt = at;
    // Only the next opening of the folder misses it.
    if (folder) setCrop(folder.path, at).catch(() => {});
  }

  /** V2, Q2: takes the folder, settings and output folder as they are now. */
  function start() {
    if (!folder || !settings || !extension || !outputDir || reason) return;
    queue.add({
      job: { folder: folder.path, settings, output: outputDir, crop: cropAt },
      name: folder.name,
      extension,
      frames: folder.names.length,
      label: settingsLine,
    });
  }

  const queued = $derived(queue.items.filter((i) => !isFinished(i.status)).length);
</script>

<svelte:window {onkeydown} />

<div class="app" style:--side="{side}px">
  <FolderBar {folder} onpick={pick} start={queue.running ? t("대기열에 추가", "Add to Queue") : t("시작", "Start")} {reason} isReady={!!folder && !!settings} onstart={start} />

  <main>
    <section class="photo">
      {#if folder && settings}
        <Preview {folder} {settings} {cropAt} oncrop={moveCrop} />
      {:else}
        <EmptyView {recent} {error} onpick={pick} onopen={load} />
      {/if}
    </section>
    <div class="handle" role="separator" aria-orientation="vertical" onpointerdown={resize}></div>
    <aside>
      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab === "settings"} class:on={tab === "settings"} onclick={() => (tab = "settings")}>{t("설정", "Settings")}</button>
        <button role="tab" aria-selected={tab === "queue"} class:on={tab === "queue"} onclick={() => (tab = "queue")}>
          {t("대기열", "Queue")}{#if queued}&nbsp;<span class="count num">{queued}</span>{/if}
        </button>
      </div>
      <div class="tab-body">
        {#if tab === "queue"}
          <!-- Y2: the only place that shows progress and results. -->
          <QueuePanel bind:openLog />
        {:else if session && profiles.length}
          <SettingsPanel {session} {profiles} {folder} {prefs} onchange={apply} />
        {/if}
      </div>
      {#if tab === "settings"}
        <div class="make">
          <small>{t("만들 영상", "Video to Make")}</small>
          {#if summary}
            <b class="num">{summary.length}</b>
            <span class="num">{summary.spec}</span>
            <span class="file num">{summary.file}</span>
          {:else}
            <span class="file">{t("폴더를 먼저 여세요", "Open a folder first")}</span>
          {/if}
          {#if reason}<p class="why warn">{reason}</p>{/if}
        </div>
      {/if}
    </aside>
  </main>

  <PowerCountdown />

  {#if dragOver}
    <div class="dropping">{t("놓으면 이 폴더를 엶", "Drop to open this folder")}</div>
  {/if}
</div>

<style>
  .app { height: 100%; display: grid; grid-template-rows: auto 1fr; }
  main { display: grid; grid-template-columns: minmax(0, 1fr) 1px var(--side); min-height: 0; }
  .photo { min-width: 0; min-height: 0; }
  /* L4: a plain line; the grab area is wider than what is drawn. */
  .handle { background: var(--line); position: relative; cursor: col-resize; }
  .handle::after { content: ""; position: absolute; inset: 0 -4px; }
  aside { background: var(--panel); min-height: 0; display: grid; grid-template-rows: auto minmax(0, 1fr) auto; grid-template-columns: minmax(0, 1fr); }
  .tabs { display: flex; gap: 2px; margin: 10px 16px 0; padding: 2px; border-radius: 8px; background: var(--hover); }
  .tabs button { flex: 1; border: 0; border-radius: 6px; padding: 3px 4px; background: none; font-size: 13px; }
  .tabs button:hover:not(.on) { background: var(--hover); }
  /* Y5 */
  .tabs .on, .tabs .on:hover { background: var(--accent); color: #fff; box-shadow: 0 1px 2px rgb(0 0 0 / 0.18); font-weight: 600; }
  .count { display: inline-block; min-width: 18px; padding: 0 5px; border-radius: 9px; background: var(--accent); color: #fff; font-size: 11px; font-weight: 600; }
  .on .count { background: #fff; color: var(--accent); }
  .tab-body { min-height: 0; }
  .make { margin: 0 16px 16px; padding: 10px 12px; border-radius: 10px; background: var(--box); border: 1px solid var(--box-line); display: grid; gap: 1px; }
  .make small { font-size: 11px; font-weight: 600; color: var(--muted); }
  .make b { font-size: 17px; }
  .make span { font-size: 12px; color: var(--muted); }
  .make .file { color: var(--faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .make .why { margin-top: 4px; }
  .dropping {
    position: fixed;
    inset: 8px;
    border: 2px dashed var(--accent);
    border-radius: calc(var(--radius) * 1.5);
    background: color-mix(in srgb, var(--accent), transparent 88%);
    display: grid;
    place-items: center;
    font-weight: 600;
    pointer-events: none;
  }
</style>
