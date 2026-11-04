<script lang="ts">
  import { untrack } from "svelte";
  import { photoTaken, previewUrl, type Folder, type Settings } from "./api";
  import { t } from "./i18n";
  import { clock, count, CROP_END, cropBox, dragCrop, fit, fpsValue, takenText, type Box } from "./format";

  /** `cropAt`: core `Settings::crop`, 0–`CROP_END`. `oncrop`: once a drag of the frame ends. */
  let { folder, settings, cropAt, oncrop }: { folder: Folder; settings: Settings; cropAt: number; oncrop: (at: number) => void } =
    $props();

  /** P5: JPG previews (~0.16 s) finish before the spinner would show. */
  const SPINNER_DELAY_MS = 300;
  /** P9: horizontal trackpad travel per photo. */
  const SWIPE_PX = 24;
  const SWIPE_SETTLE_MS = 250;
  const STAGE_PADDING = 24;

  type Shown = { index: number; url: string; w: number; h: number };

  let index = $state(0);
  /** P4: slider, key or swipe moving: only the number follows until it settles. */
  let scrubbing = $state(false);
  let shown = $state<Shown | null>(null);
  let failed = $state<{ index: number; reason: string } | null>(null);
  let spinner = $state(false);
  let taken = $state<string | null>(null);
  /** P11: blur-free version of the shown photo while [원본] is held. */
  let original = $state<{ index: number; url: string } | null>(null);
  let holdingOriginal = $state(false);
  let typing = $state(false);
  let stageW = $state(0);
  let stageH = $state(0);
  /** Where the pointer went down on the crop frame, and the frame then. */
  let drag = $state<{ x: number; y: number; from: Box } | null>(null);
  let dragAt = $state<number | null>(null);

  let request = 0;
  let swipe = 0;
  let swipeTimer: ReturnType<typeof setTimeout> | undefined;

  const total = $derived(folder.names.length);
  const fps = $derived(fpsValue(settings.fps));
  const crop = $derived(shown && cropBox(shown.w, shown.h, settings.width, settings.height, dragAt ?? cropAt / CROP_END));
  const box = $derived.by(() => {
    const [w, h] = shown ? [shown.w, shown.h] : [settings.width, settings.height];
    return fit(w, h, Math.max(stageW - STAGE_PADDING * 2, 1), Math.max(stageH - STAGE_PADDING * 2, 1));
  });
  const fraction = $derived(total > 1 ? index / (total - 1) : 0);

  // A new folder starts at its first photo.
  $effect(() => {
    folder;
    untrack(() => {
      index = 0;
      load(0);
    });
  });

  // Blur changes the picture itself; size and fps only move the crop and the clock.
  let blurLoaded = untrack(() => settings.blur);
  $effect(() => {
    if (settings.blur === blurLoaded) return;
    blurLoaded = settings.blur;
    untrack(() => load(index));
  });

  const decodeSize = (url: string) =>
    new Promise<{ w: number; h: number }>((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve({ w: img.naturalWidth, h: img.naturalHeight });
      img.onerror = () => reject(new Error(t("미리보기 그림이 깨졌음", "The preview image is broken")));
      img.src = url;
    });

  async function load(i: number) {
    const id = ++request;
    scrubbing = false;
    clearOriginal();
    const spinnerTimer = setTimeout(() => id === request && (spinner = true), SPINNER_DELAY_MS);
    photoTaken(i).then(
      (t) => id === request && (taken = t),
      () => id === request && (taken = null),
    );
    try {
      const url = await previewUrl(i, false);
      const size = await decodeSize(url);
      if (id !== request) return URL.revokeObjectURL(url);
      if (shown) URL.revokeObjectURL(shown.url);
      shown = { index: i, url, ...size };
      failed = null;
    } catch (e) {
      if (id !== request) return;
      failed = { index: i, reason: String(e) };
    } finally {
      clearTimeout(spinnerTimer);
      if (id === request) spinner = false;
    }
  }

  function clearOriginal() {
    if (original) URL.revokeObjectURL(original.url);
    original = null;
    holdingOriginal = false;
  }

  async function holdOriginal() {
    if (!shown) return;
    holdingOriginal = true;
    if (original?.index === shown.index) return;
    const target = shown.index;
    try {
      const url = await previewUrl(target, true);
      if (shown?.index !== target) return URL.revokeObjectURL(url);
      original = { index: target, url };
    } catch {
      holdingOriginal = false;
    }
  }

  function startDrag(e: PointerEvent & { currentTarget: HTMLElement }) {
    if (!crop) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, from: crop };
  }

  function moveDrag(e: PointerEvent) {
    if (drag) dragAt = dragCrop(drag.from, (e.clientX - drag.x) / box.w, (e.clientY - drag.y) / box.h);
  }

  function endDrag() {
    if (dragAt !== null) oncrop(Math.round(dragAt * CROP_END));
    drag = null;
    dragAt = null;
  }

  const clamp = (i: number) => Math.min(Math.max(i, 0), total - 1);

  function move(to: number) {
    index = clamp(to);
    scrubbing = true;
  }

  const STEP_KEYS: Record<string, (shift: boolean) => number> = {
    ArrowLeft: (shift) => index - (shift ? 10 : 1),
    ArrowRight: (shift) => index + (shift ? 10 : 1),
    Home: () => 0,
    End: () => total - 1,
  };

  function onkeydown(e: KeyboardEvent) {
    const step = STEP_KEYS[e.key];
    if (!step || typing || e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault();
    move(step(e.shiftKey));
  }

  // Held keys only move the number; the photo is read once the key comes up.
  function onkeyup(e: KeyboardEvent) {
    if (STEP_KEYS[e.key] && scrubbing && !typing) load(index);
  }

  function onwheel(e: WheelEvent) {
    if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
    swipe += e.deltaX;
    const steps = Math.trunc(swipe / SWIPE_PX);
    if (steps) {
      swipe -= steps * SWIPE_PX;
      move(index + steps);
    }
    clearTimeout(swipeTimer);
    swipeTimer = setTimeout(() => scrubbing && load(index), SWIPE_SETTLE_MS);
  }

  function jumpTo(e: Event & { currentTarget: HTMLInputElement }) {
    const n = Number(e.currentTarget.value);
    typing = false;
    if (Number.isInteger(n) && n >= 1) {
      index = clamp(n - 1);
      load(index);
    }
  }

  const videoTime = (i: number) => clock(i / fps);
</script>

<svelte:window {onkeydown} {onkeyup} />

<div class="preview">
  <div class="stage" bind:clientWidth={stageW} bind:clientHeight={stageH} {onwheel}>
    {#if failed || shown}
      <div class="frame" class:failed style:width="{box.w}px" style:height="{box.h}px">
        {#if failed}
          <!-- P10: the box says which photo failed and why. -->
          <strong>⚠ {t("못 읽음", "Can’t Read")}</strong>
          <span>{folder.names[failed.index]}</span>
          <span class="reason">{failed.reason}</span>
        {:else if shown}
          <img src={holdingOriginal && original ? original.url : shown.url} alt={folder.names[shown.index]} draggable="false" />
          {#if crop}
            <!-- P6/P7: what the video keeps; the rest is dimmed. Dragged along the side with room. -->
            <div
              class="crop"
              role="presentation"
              title={t("끌어서 위치 옮기기 · 더블클릭 = 가운데", "Drag to move · Double-click to center")}
              style:cursor={crop.w < 1 ? "ew-resize" : "ns-resize"}
              onpointerdown={startDrag}
              onpointermove={moveDrag}
              onpointerup={endDrag}
              onpointercancel={endDrag}
              ondblclick={() => oncrop(CROP_END / 2)}
              style:left="{crop.x * 100}%"
              style:top="{crop.y * 100}%"
              style:width="{crop.w * 100}%"
              style:height="{crop.h * 100}%"
            ></div>
          {/if}
          {#if settings.blur > 0}
            <!-- P11 -->
            <div class="blur">
              <span>{t(`블러 ${settings.blur} · ${settings.blur}장 섞음`, `Blur ${settings.blur} · ${settings.blur} photos mixed`)}</span>
              <button
                onpointerdown={holdOriginal}
                onpointerup={() => (holdingOriginal = false)}
                onpointerleave={() => (holdingOriginal = false)}>{t("원본", "Original")}</button
              >
            </div>
          {/if}
        {/if}
        {#if scrubbing}
          <div class="scrub num">{count(index + 1)}</div>
        {/if}
      </div>
    {/if}
    {#if spinner}<div class="spinner" aria-label={t("읽는 중", "Loading")}></div>{/if}
  </div>

  <!-- P2: full-width band at the bottom of the photo pane. -->
  <div class="band">
    <div class="track" style:--fraction={fraction}>
      <input
        type="range"
        min="0"
        max={total - 1}
        bind:value={index}
        oninput={() => (scrubbing = true)}
        onchange={() => load(index)}
        aria-label={t("사진 번호", "Photo number")}
      />
      <!-- P1: the number rides on the thumb. -->
      <span class="thumb num" aria-hidden="true">{count(index + 1)}</span>
    </div>
    <div class="info">
      {#if typing}
        <input
          class="jump num"
          type="number"
          min="1"
          max={total}
          placeholder={String(index + 1)}
          onchange={jumpTo}
          onkeydown={(e) => e.key === "Escape" && (typing = false)}
          onblur={() => (typing = false)}
          {@attach (el) => el.focus()}
        />
      {:else}
        <button class="where num" title={t("번호 입력", "Type a number")} onclick={() => (typing = true)}>
          {count(index + 1)} / {count(total)} · {videoTime(index)} / {videoTime(total)}
        </button>
      {/if}
      {#if failed && !scrubbing}
        <span class="photo num">{folder.names[failed.index]}</span>
      {:else if shown && !scrubbing}
        <!-- P8 -->
        <span class="photo num">{folder.names[shown.index]}{taken ? ` · ${takenText(taken)}` : ""}</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .preview { height: 100%; display: grid; grid-template-rows: 1fr auto; min-width: 0; min-height: 0; }
  .stage { position: relative; display: grid; place-items: center; overflow: hidden; min-height: 0; }
  .frame { position: relative; overflow: hidden; border-radius: 4px; }
  img { display: block; width: 100%; height: 100%; }
  .crop {
    position: absolute;
    outline: 1.5px dashed rgb(255 255 255 / 0.9);
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.45);
    touch-action: none;
  }
  .blur {
    position: absolute;
    top: 10px;
    left: 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: #fff;
  }
  .blur span { background: rgb(0 0 0 / 0.55); padding: 3px 9px; border-radius: 999px; }
  .blur button { font-size: 12px; padding: 2px 10px; background: rgb(0 0 0 / 0.55); color: #fff; border-color: rgb(255 255 255 / 0.3); }
  .blur button:active { background: var(--accent); }
  .scrub {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.55);
    color: #fff;
    font-size: 56px;
    font-weight: 700;
  }
  .failed {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 6px;
    padding: 16px;
    background: var(--hover);
    border: 1px solid var(--line);
    text-align: center;
  }
  .failed strong { color: var(--warn); }
  .reason { color: var(--muted); font-size: 12px; max-width: 46ch; user-select: text; }
  .spinner {
    position: absolute;
    top: 14px;
    right: 14px;
    width: 18px;
    height: 18px;
    border: 2px solid var(--line);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(1turn); } }

  .band { padding: 8px 16px 10px; border-top: 1px solid var(--line); background: var(--panel); display: grid; gap: 4px; }
  .track { --thumb: 64px; position: relative; height: 26px; }
  input[type="range"] {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 26px;
    margin: 0;
    background: transparent;
  }
  input[type="range"]::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) calc(var(--fraction) * 100%), var(--line) 0);
  }
  /* The real thumb is invisible; .thumb draws it with the number on top. */
  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: var(--thumb);
    height: 22px;
    margin-top: -9px;
    opacity: 0;
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: calc(var(--fraction) * (100% - var(--thumb)));
    width: var(--thumb);
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 999px;
    background: var(--raised);
    border: 1px solid var(--line);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
    font-size: 12px;
    font-weight: 600;
    pointer-events: none;
  }
  .info { display: flex; justify-content: space-between; align-items: center; gap: 12px; min-height: 26px; font-size: 12px; color: var(--muted); }
  .where { border: 0; background: none; padding: 2px 6px; margin-left: -6px; color: var(--muted); font-size: 12px; }
  .where:hover { background: var(--hover); color: var(--text); }
  .jump { width: 9ch; font: inherit; font-size: 12px; padding: 2px 6px; border-radius: 6px; border: 1px solid var(--accent); background: var(--raised); color: var(--text); }
  .photo { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
</style>
