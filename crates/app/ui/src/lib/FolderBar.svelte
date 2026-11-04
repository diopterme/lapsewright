<script lang="ts">
  import type { Folder } from "./api";
  import { count, period } from "./format";
  import { t } from "./i18n";

  let { folder, onpick, start, reason, isReady, onstart }: {
    folder: Folder | null;
    onpick: () => void;
    /** [시작], or [대기열에 추가] while one is converting. */
    start: string;
    /** B1: why [시작] can't be pressed; the summary under the panel says it too. */
    reason: string | null;
    isReady: boolean;
    onstart: () => void;
  } = $props();

  const meta = $derived(
    folder
      ? [t(`${count(folder.names.length)}장`, `${count(folder.names.length)} photos`), period(folder.firstTaken, folder.lastTaken), folder.formats.join(" · ")]
          .filter(Boolean)
          .join(" · ")
      : "",
  );
</script>

<!-- Sits in the title bar (L1): empty parts drag the window. Y1: the main action at the right end. -->
<header data-tauri-drag-region>
  <div class="title" data-tauri-drag-region>
    <b title={folder?.path} data-tauri-drag-region>{folder?.name ?? "Lapsewright"}</b>
    {#if folder}<span class="meta num" data-tauri-drag-region>{meta}</span>{/if}
  </div>
  {#if folder}
    <button class="tool" onclick={onpick}>
      <svg viewBox="0 0 20 16" aria-hidden="true">
        <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h4.6l1.6 1.8h8.8A1.5 1.5 0 0 1 19 5.3v8.2a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 1 13.5z" />
      </svg>
      {t("열기…", "Open…")}
    </button>
  {/if}
  <button class="primary go" disabled={!isReady || !!reason} title={reason ?? "⌘↩"} onclick={onstart}>
    <svg viewBox="0 0 10 12" aria-hidden="true"><path d="M1 1v10l8-5z" /></svg>
    {start}
  </button>
</header>

<style>
  header {
    height: 52px;
    /* Room for the traffic lights (tauri.conf.json trafficLightPosition). */
    padding: 0 12px 0 88px;
    display: flex;
    align-items: center;
    gap: 8px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
    min-width: 0;
  }
  .title { flex: 1; min-width: 0; display: grid; line-height: 1.25; }
  .title b, .meta { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { color: var(--muted); font-size: 12px; }
  button { flex: none; display: inline-flex; align-items: center; gap: 6px; }
  .tool { border-color: transparent; background: none; padding: 4px 10px; border-radius: 7px; }
  .tool:hover:not(:disabled) { background: var(--hover); }
  .tool svg { width: 16px; fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linejoin: round; }
  .go { padding: 5px 18px; border-radius: 999px; font-weight: 600; }
  .go svg { width: 9px; fill: currentColor; }
</style>
