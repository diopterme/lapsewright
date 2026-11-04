<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { clearRecent, copyrightText, loadPrefs, openLink, setPrefs, type Power, type Prefs } from "./lib/api";
  import { t } from "./lib/i18n";
  import Segmented from "./lib/Segmented.svelte";

  const REPO = "https://github.com/diopterme/lapsewright";
  const SPONSOR = "https://github.com/sponsors/diopterme";
  const FFMPEG = "https://ffmpeg.org/";

  const POWERS: { value: Power; label: string }[] = [
    { value: "none", label: t("안 함", "Nothing") },
    { value: "sleep", label: t("잠자기", "Sleep") },
    { value: "shutdown", label: t("시스템 종료", "Shut Down") },
  ];

  type Tab = "general" | "finish" | "about";
  const TABS: { value: Tab; label: string }[] = [
    { value: "general", label: t("일반", "General") },
    { value: "finish", label: t("끝났을 때", "When Done") },
    { value: "about", label: t("정보", "About") },
  ];

  /** W2 */
  let tab = $state<Tab>("general");

  /** Y11: the window is named after the pane, as in mac settings windows. */
  $effect(() => {
    const label = TABS.find((t) => t.value === tab)?.label ?? t("설정", "Settings");
    getCurrentWindow().setTitle(label).catch((e) => (error = String(e)));
  });
  let prefs = $state<Prefs | null>(null);
  /** M1: what the video gets. */
  let copyright = $state("");
  let isCleared = $state(false);
  let error = $state<string | null>(null);
  let version = $state("");

  onMount(() => {
    loadPrefs().then((p) => (prefs = p), (e) => (error = String(e)));
    getVersion().then((v) => (version = v), (e) => (error = String(e)));
  });

  /** W1: every change is saved and used at once. */
  async function change(patch: Partial<Prefs>) {
    if (!prefs) return;
    prefs = { ...prefs, ...patch };
    try {
      await setPrefs(prefs);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    const holder = prefs?.copyright.trim() ?? "";
    copyright = "";
    if (holder) copyrightText(holder).then((t) => holder === prefs?.copyright.trim() && (copyright = t), (e) => (error = String(e)));
  });

  /** W4 */
  async function forget() {
    try {
      await clearRecent();
      isCleared = true;
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  function open(url: string) {
    openLink(url).then(() => (error = null), (e) => (error = String(e)));
  }
</script>

<div class="window">
  <!-- Y11: icon tabs on top. -->
  <div class="icons" role="tablist" aria-label={t("설정", "Settings")}>
    <button role="tab" aria-selected={tab === "general"} class:on={tab === "general"} onclick={() => (tab = "general")}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="3.2" /><circle cx="12" cy="12" r="6.5" />
        <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" />
      </svg>
      {TABS[0].label}
    </button>
    <button role="tab" aria-selected={tab === "finish"} class:on={tab === "finish"} onclick={() => (tab = "finish")}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z" /><path d="M10 20.5a2 2 0 0 0 4 0" /></svg>
      {TABS[1].label}
    </button>
    <button role="tab" aria-selected={tab === "about"} class:on={tab === "about"} onclick={() => (tab = "about")}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5" /><path d="M12 11v5.5M12 7.5v.5" /></svg>
      {TABS[2].label}
    </button>
  </div>

  <div class="body">
    {#if tab === "about"}
      <section class="about">
        <h3>Lapsewright</h3>
        <p class="why">{t("버전", "Version")} {version} · diopterme</p>
        <p>{t("사진 폴더를 타임랩스 영상으로.", "Turns a folder of photos into a timelapse video.")}</p>
        <div class="links">
          <button class="mini" onclick={() => open(REPO)}>{t("소스 코드", "Source Code")}</button>
          <button class="mini" onclick={() => open(SPONSOR)}>♥ {t("후원하기", "Sponsor")}</button>
        </div>
      </section>
      <section class="group">
        <h4>{t("함께 들어 있는 것", "Included")}</h4>
        <div class="box">
          <div class="row">
            <span>FFmpeg</span>
            <span class="value"><button class="mini" onclick={() => open(FFMPEG)}>ffmpeg.org</button></span>
            <p class="cap">
              {t(
                "사진을 영상으로 바꾸는 프로그램. FFmpeg 개발자들이 만듦 (GPLv3). FFmpeg 원본 코드는 이 앱을 내려받은 곳(GitHub Releases)에 같이 있음.",
                "Turns the photos into the video. Made by the FFmpeg developers, licensed under GPLv3. Its source code is on the page this app was downloaded from (GitHub Releases).",
              )}
            </p>
          </div>
        </div>
      </section>
      <p class="why">
        {t(
          "Lapsewright 도 GPLv3: 누구나 쓰고, 고치고, 나눌 수 있음. 아무 보증 없음.",
          "Lapsewright is free software under GPLv3: anyone may use, change and share it. It comes with no warranty.",
        )}
      </p>
    {:else if prefs}
      {#if tab === "general"}
        <section class="group">
          <h4>{t("영상에 넣는 이름", "Names in the Video")}</h4>
          <div class="box">
            <label class="row">
              <span>{t("작성자", "Author")}</span>
              <input value={prefs.author} placeholder={t("비우면 사진 EXIF 것", "Empty: the photo's EXIF")} oninput={(e) => change({ author: e.currentTarget.value })} />
            </label>
            <label class="row">
              <span>{t("저작권자", "Copyright")}</span>
              <input value={prefs.copyright} placeholder={t("비우면 사진 EXIF 것", "Empty: the photo's EXIF")} oninput={(e) => change({ copyright: e.currentTarget.value })} />
            </label>
          </div>
          <p class="why">
            {t("영상에:", "In the video:")}
            {copyright || t("사진 EXIF 의 저작권 (없으면 안 넣음)", "the photo's EXIF copyright (none if it has none)")}{#if copyright}{t(" · 연도는 사진 찍은 해", " · the year the photos were taken")}{/if}
          </p>
        </section>
        <section class="group">
          <h4>{t("최근 폴더", "Recent Folders")}</h4>
          <div class="box">
            <div class="row">
              <span>{t("처음 화면 목록", "List on the start screen")}</span>
              <span class="value">
                {#if isCleared}<span class="why">{t("지움 ✓", "Cleared ✓")}</span>{/if}
                <button class="mini" onclick={forget}>{t("지우기", "Clear")}</button>
              </span>
            </div>
          </div>
          <p class="why">{t("목록만 지움. 폴더와 사진은 그대로.", "Only the list. Folders and photos stay.")}</p>
        </section>
      {:else}
        <section class="group">
          <h4>{t("하나 끝날 때마다", "After Each Video")}</h4>
          <div class="box">
            <div class="row"><span>{t("알림 + 소리", "Notification + sound")}</span><span class="why">{t("완성 Glass · 실패 Basso", "Done: Glass · Failed: Basso")}</span></div>
          </div>
        </section>
        <section class="group">
          <h4>{t("대기열이 다 끝나면 (마지막 영상)", "When the Queue Is Done (Last Video)")}</h4>
          <div class="box">
            <label class="row"><span>{t("Finder 에서 보기", "Show in Finder")}</span><input type="checkbox" checked={prefs.reveal} onchange={(e) => change({ reveal: e.currentTarget.checked })} /></label>
            <label class="row"><span>{t("영상 바로 열기", "Open the video")}</span><input type="checkbox" checked={prefs.open} onchange={(e) => change({ open: e.currentTarget.checked })} /></label>
          </div>
        </section>
        <section class="group">
          <h4>{t("그다음 맥", "Then the Mac")}</h4>
          <div class="box">
            <div class="row"><span>{t("끝나면", "When done")}</span><Segmented label={t("그다음 맥", "Then the Mac")} options={POWERS} value={prefs.power} onpick={(power) => change({ power })} /></div>
          </div>
          <p class="why">{t("1분 세고 함 ([취소] 가능). 실패나 취소가 하나라도 있으면 안 함.", "After a 1-minute countdown ([Cancel] stops it). Skipped if any video failed or was cancelled.")}</p>
          {#if prefs.power === "shutdown"}
            <p class="why">{t('처음 한 번 맥이 "Lapsewright 가 System Events 를 제어" 해도 되는지 물음 → 허용.', 'The first time, macOS asks whether "Lapsewright" may control "System Events". Allow it.')}</p>
          {/if}
        </section>
      {/if}
    {/if}

    {#if error}<p class="why warn">{error}</p>{/if}
  </div>
</div>

<style>
  .window { height: 100%; display: grid; grid-template-rows: auto minmax(0, 1fr); background: var(--panel); }
  .icons { display: flex; justify-content: center; gap: 4px; padding: 8px 0; border-bottom: 1px solid var(--line); }
  .icons button { display: grid; justify-items: center; gap: 1px; border: 0; background: none; padding: 4px 12px; border-radius: 7px; font-size: 11px; color: var(--muted); }
  .icons button:hover:not(:disabled) { background: var(--hover); }
  .icons .on, .icons .on:hover:not(:disabled) { background: var(--control); color: var(--text); }
  svg { width: 24px; height: 22px; fill: none; stroke: currentColor; stroke-width: 1.5; }
  .on svg { stroke: var(--accent); }
  .body { display: flex; flex-direction: column; gap: 16px; padding: 16px 20px 20px; overflow-y: auto; }
  .group .why { margin: 0 4px; }
  input:not([type]) { width: 220px; font: inherit; padding: 3px 8px; border-radius: 6px; border: 1px solid var(--line); background: var(--win); color: var(--text); user-select: text; }
  .about { display: grid; justify-items: center; gap: 4px; text-align: center; }
  .about h3 { margin: 0; font-size: 18px; }
  .about p { margin: 0; }
  .links { display: flex; gap: 8px; margin-top: 6px; }
  .row > .cap { line-height: 1.4; padding-bottom: 4px; }
  .body > .why { margin: 0 4px; }
</style>
