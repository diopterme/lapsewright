<script lang="ts">
  import { openSettings, type Folder, type Prefs, type Profile, type Session, type Settings } from "./api";
  import { fpsText } from "./format";
  import {
    BLURS,
    CHROMAS,
    FPS_LIST,
    MAX_SIDE,
    RANGES,
    RANGE_TEXT,
    SIZES,
    USES,
    advancedSummary,
    chromaText,
    chromaWhy,
    findProfile,
    hdrWhy,
    isValidSide,
    profileLine,
    replaced,
    switchUse,
  } from "./settings";
  import Segmented from "./Segmented.svelte";
  import QualitySlider from "./QualitySlider.svelte";
  import PresetMenu from "./PresetMenu.svelte";
  import OutputField from "./OutputField.svelte";
  import { t } from "./i18n";

  let { session, profiles, folder, prefs, onchange }: {
    session: Session;
    profiles: Profile[];
    folder: Folder | null;
    prefs: Prefs | null;
    onchange: (next: Session) => Promise<Session>;
  } = $props();

  const ADVANCED_KEY = "lapsewright.advancedOpen";
  const FLASH_MS = 1500;
  const CUSTOM = "custom";

  /** C8: remembered like the panel width. */
  let advancedOpen = $state(readAdvanced());
  /** C3: what the backend replaced, until the next change. */
  let notice = $state("");
  let flashing = $state<(keyof Settings)[]>([]);
  let error = $state<string | null>(null);
  /** C4: "직접 입력…" picked, or a size not in the list. */
  let typingSize = $state(false);
  let sizeProblem = $state(false);
  let widthInput = $state<HTMLInputElement>();
  let heightInput = $state<HTMLInputElement>();
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  const s = $derived(session.settings);
  const profile = $derived(findProfile(profiles, s));
  const codecProfiles = $derived(profiles.filter((p) => p.codec === s.codec));
  const isListedSize = $derived(SIZES.some(([w, h]) => w === s.width && h === s.height));
  const fpsOptions = $derived(FPS_LIST.includes(s.fps) ? FPS_LIST : [...FPS_LIST, s.fps]);

  function readAdvanced(): boolean {
    try {
      return localStorage.getItem(ADVANCED_KEY) === "1";
    } catch {
      return false;
    }
  }

  function toggleAdvanced() {
    advancedOpen = !advancedOpen;
    try {
      localStorage.setItem(ADVANCED_KEY, advancedOpen ? "1" : "0");
    } catch {
      // Only the folding is forgotten.
    }
  }

  async function change(asked: Settings) {
    try {
      const got = await onchange({ ...session, settings: asked });
      const { fields, text } = replaced(asked, got.settings);
      notice = text;
      flashing = fields;
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (flashing = []), FLASH_MS);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  const set = (patch: Partial<Settings>) => change({ ...s, ...patch });

  function pickSize(e: Event & { currentTarget: HTMLSelectElement }) {
    const value = e.currentTarget.value;
    typingSize = value === CUSTOM;
    sizeProblem = false;
    if (value === CUSTOM) return;
    const [width, height] = value.split("x").map(Number);
    set({ width, height });
  }

  function typeSize() {
    const [width, height] = [Number(widthInput?.value), Number(heightInput?.value)];
    sizeProblem = !isValidSide(width) || !isValidSide(height);
    if (!sizeProblem) set({ width, height });
  }

  const flash = (...keys: (keyof Settings)[]) => keys.some((k) => flashing.includes(k));
</script>

<div class="panel">
  <PresetMenu {session} {onchange} />

  <!-- Y4 -->
  <section class="group">
    <h4>{t("형식", "Format")}</h4>
    <div class="box">
      <div class="row">
        <span>{t("코덱", "Codec")}</span>
        <Segmented
          label={t("코덱", "Codec")}
          options={USES.map((u) => ({ value: u.codec, label: u.label, title: u.title }))}
          value={s.codec}
          onpick={(codec) => change(switchUse(s, codec, profiles))}
        />
        <p class="cap">{USES.find((u) => u.codec === s.codec)?.title}</p>
      </div>
    </div>
  </section>

  <section class="group">
    <h4>{t("크기 · 속도", "Size · Speed")}</h4>
    <div class="box">
      <div class="row">
        <span>{t("크기", "Size")}</span>
        <select value={typingSize || !isListedSize ? CUSTOM : `${s.width}x${s.height}`} onchange={pickSize} aria-label={t("크기", "Size")}>
          {#each SIZES as [w, h] (w)}<option value="{w}x{h}">{w}×{h}</option>{/each}
          <option value={CUSTOM}>{t("직접 입력…", "Custom…")}</option>
        </select>
        {#if typingSize || !isListedSize}
          <div class="size num">
            <input bind:this={widthInput} type="number" min="2" max={MAX_SIDE} step="2" value={s.width} onchange={typeSize} aria-label={t("가로", "Width")} />
            <span>×</span>
            <input bind:this={heightInput} type="number" min="2" max={MAX_SIDE} step="2" value={s.height} onchange={typeSize} aria-label={t("세로", "Height")} />
          </div>
          {#if sizeProblem}<p class="cap warn">{t(`짝수, ${MAX_SIDE} 이하`, `Even numbers, up to ${MAX_SIDE}`)}</p>{/if}
        {/if}
      </div>
      <div class="row">
        <span>fps</span>
        <select value={s.fps} onchange={(e) => set({ fps: e.currentTarget.value })} aria-label="fps">
          {#each fpsOptions as fps (fps)}<option value={fps}>{fpsText(fps)}</option>{/each}
        </select>
      </div>
      <div class="row">
        <span>{t("모션 블러", "Motion Blur")}</span>
        <Segmented
          label={t("모션 블러", "Motion Blur")}
          options={BLURS.map((b) => ({ value: b, label: b ? String(b) : t("끔", "Off"), title: b ? t(`${b}장 섞음`, `${b} photos mixed`) : undefined }))}
          value={s.blur}
          onpick={(blur) => set({ blur })}
        />
      </div>
    </div>
  </section>

  <div class="box">
    <button class="row fold" aria-expanded={advancedOpen} onclick={toggleAdvanced}>
      <span>{advancedOpen ? "▾" : "▸"} {t("고급", "Advanced")}</span>
      {#if !advancedOpen}<span class="summary" class:flash={flashing.length > 0}>{advancedSummary(s)}</span>{/if}
    </button>

    {#if advancedOpen && profile}
      <div class="row">
        <span>{t("프로파일", "Profile")}</span>
        <select value={s.profile} onchange={(e) => set({ profile: e.currentTarget.value })} aria-label={t("프로파일", "Profile")}>
          {#each codecProfiles as p (p.id)}<option value={p.id}>{profileLine(p)}</option>{/each}
        </select>
      </div>

      <div class="row" class:flash={flash("chroma")}>
        <span>{t("크로마", "Chroma")}</span>
        <Segmented
          label={t("크로마", "Chroma")}
          options={CHROMAS.map((c) => ({ value: c, label: chromaText(c), disabled: !profile.chroma.includes(c) }))}
          value={s.chroma}
          onpick={(chroma) => set({ chroma })}
        />
        {#if chromaWhy(profile)}<p class="cap">{chromaWhy(profile)}</p>{/if}
      </div>

      <div class="row" class:flash={flash("range")}>
        <span>{t("밝기 범위", "Range")}</span>
        <Segmented
          label={t("밝기 범위", "Range")}
          options={RANGES.map((r) => ({ value: r, label: RANGE_TEXT[r], disabled: r !== "sdr" && !profile.hdr }))}
          value={s.range}
          onpick={(range) => set({ range })}
        />
        {#if hdrWhy(profiles, profile)}<p class="cap">{hdrWhy(profiles, profile)}</p>{/if}
      </div>

      <div class="row quality" class:flash={flash("crf")}>
        <QualitySlider crf={s.crf} disabled={s.codec === "prores"} onpick={(crf) => set({ crf })} />
      </div>
    {/if}
  </div>
  {#if notice}<p class="why notice">{notice}</p>{/if}
  {#if error}<p class="why warn">{error}</p>{/if}

  <section class="group">
    <h4>{t("저장", "Save")}</h4>
    <div class="box">
      <OutputField output={session.output} {folder} onpick={(output) => onchange({ ...session, output }).catch((e) => (error = String(e)))} />
      {#if prefs}
        <!-- M3 -->
        <div class="row">
          <span>{t("작성자", "Author")}</span>
          <button class="credits" title={t("설정 창(⌘,)에서 바꿈", "Change in Settings (⌘,)")} onclick={() => openSettings().catch((e) => (error = String(e)))}>
            {prefs.author.trim() || t("사진 EXIF", "Photo EXIF")} · © {prefs.copyright.trim() || t("사진 EXIF", "Photo EXIF")}
          </button>
        </div>
      {/if}
    </div>
  </section>
</div>

<style>
  .panel { height: 100%; display: flex; flex-direction: column; gap: 14px; padding: 14px 16px 16px; overflow-y: auto; }
  .row select { max-width: 62%; }
  .size { flex-basis: 100%; display: flex; justify-content: flex-end; align-items: center; gap: 6px; padding-bottom: 2px; }
  .size input { width: 7ch; }
  .fold { width: 100%; border: 0; background: none; border-radius: 10px; text-align: left; font-weight: 600; flex-wrap: nowrap; }
  .fold:hover:not(:disabled) { background: none; }
  .summary { font-weight: 400; color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; border-radius: 4px; }
  .quality { display: block; padding-block: 8px; }
  .notice { color: var(--accent); }
  /* C3: a replaced value glows once. */
  .flash { animation: flash 1.5s ease-out; }
  @keyframes flash { from { background: color-mix(in srgb, var(--accent), transparent 70%); } }
  .credits { min-width: 0; border: 0; background: none; padding: 0; font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .credits:hover:not(:disabled) { background: none; text-decoration: underline; }
</style>
