<script lang="ts">
  import { openVideo, reveal } from "./api";
  import { bytesText, spanText } from "./format";
  import { isFinished, percent, type Item } from "./queue.ts";
  import { queue } from "./queue.svelte.ts";
  import { t } from "./i18n";

  /** The item whose log is unfolded; kept by the window across tab switches. */
  let { openLog = $bindable(null) }: { openLog: number | null } = $props();

  let list = $state<HTMLElement>();
  let error = $state<string | null>(null);
  let copied = $state<number | null>(null);

  const COPIED_MS = 1500;
  const took = (i: Item) => (i.started && i.ended ? spanText((i.ended - i.started) / 1000) : "");
  const fileName = (path: string | null) => path?.split("/").pop() ?? "";

  function status(i: Item): string {
    switch (i.status) {
      case "waiting": return t("기다림", "Waiting");
      case "running": return `${percent(i)}%`;
      case "done": return `✓ ${bytesText(i.size ?? 0)} · ${took(i)}`;
      case "warn": return t(`⚠ ${bytesText(i.size ?? 0)} · 정보 일부 못 넣음`, `⚠ ${bytesText(i.size ?? 0)} · Some info not added`);
      case "failed": return `✕ ${i.message ?? t("실패", "Failed")}`;
      case "cancelled": return t("취소됨", "Cancelled");
    }
  }

  /** Q4: drag by ≡; the item goes before the row under the pointer. */
  function grab(e: PointerEvent, id: number) {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const onmove = (m: PointerEvent) => {
      const rows = [...(list?.querySelectorAll<HTMLElement>("[data-row]") ?? [])];
      const from = queue.items.findIndex((i) => i.id === id);
      const below = rows.findIndex((r) => m.clientY < r.getBoundingClientRect().top + r.offsetHeight / 2);
      const to = below === -1 ? rows.length - 1 : below > from ? below - 1 : below;
      queue.reorder(from, to);
    };
    handle.addEventListener("pointermove", onmove);
    handle.addEventListener("pointerup", () => handle.removeEventListener("pointermove", onmove), { once: true });
  }

  async function copy(i: Item) {
    try {
      await navigator.clipboard.writeText(i.log.join("\n"));
      copied = i.id;
      setTimeout(() => copied === i.id && (copied = null), COPIED_MS);
      error = null;
    } catch (e) {
      error = t(`복사 못 함: ${e}`, `Couldn’t copy: ${e}`);
    }
  }

  const run = (action: Promise<void>) => action.then(() => (error = null), (e) => (error = String(e)));
  const show = (path: string | null) => run(reveal(path ?? ""));
  const play = (path: string | null) => run(openVideo(path ?? ""));
</script>

<div class="queue">
  {#if queue.items.length === 0}
    <p class="empty">{t("비어 있음. [시작] 을 누르면 여기 섬.", "Empty. Videos line up here when you press [Start].")}</p>
  {/if}
  <ol bind:this={list}>
    {#each queue.items as i (i.id)}
      <li data-row class={i.status}>
        <div class="line">
          {#if i.status === "waiting"}
            <span class="grip" role="button" tabindex="-1" aria-label={t("끌어서 순서 바꾸기", "Drag to reorder")} onpointerdown={(e) => grab(e, i.id)}>≡</span>
          {/if}
          <span class="what">
            <b>{i.status === "waiting" ? `${i.name}.${i.extension}` : fileName(i.file) || i.name}</b>
            <small>{i.status === "waiting" ? t(`${i.label} · 있으면 _2`, `${i.label} · _2 if taken`) : i.label}</small>
          </span>
        </div>
        <!-- T3: name and description get the whole first line; status and button go below. -->
        <div class="line">
          <span class="st num" title={i.message ?? undefined}>{status(i)}</span>
          {#if i.status === "running"}
            <button onclick={() => queue.askCancel()}>{t("취소", "Cancel")}</button>
          {:else}
            <button class="x" aria-label={t("빼기", "Remove")} onclick={() => queue.remove(i.id)}>×</button>
          {/if}
        </div>
        {#if i.status === "running"}
          <div class="bar"><i style:width="{percent(i)}%"></i></div>
        {/if}
        {#if isFinished(i.status)}
          <div class="acts">
            {#if i.status === "done" || i.status === "warn"}<button onclick={() => show(i.file)}>Finder</button>{/if}
            <!-- Y2: was on the bottom band. -->
            {#if i.status === "done"}<button onclick={() => play(i.file)}>{t("열기", "Open")}</button>{/if}
            {#if i.status === "failed" || i.status === "cancelled"}<button onclick={() => queue.retry(i.id)}>{t("다시 넣기", "Retry")}</button>{/if}
            {#if i.log.length}
              <button aria-expanded={openLog === i.id} onclick={() => (openLog = openLog === i.id ? null : i.id)}>
                {openLog === i.id ? "▾" : "▸"} {t("작업 기록", "Log")}
              </button>
            {/if}
          </div>
          {#if openLog === i.id}
            <div class="log">
              <button class="copy" onclick={() => copy(i)}>{copied === i.id ? t("복사됨", "Copied") : t("복사", "Copy")}</button>
              <pre>{i.log.join("\n")}</pre>
            </div>
          {/if}
        {/if}
      </li>
    {/each}
  </ol>
  {#if error}<p class="why warn">{error}</p>{/if}
  {#if queue.items.some((i) => isFinished(i.status))}
    <button class="clear" onclick={() => queue.clearFinished()}>{t("끝난 것 지우기", "Clear Finished")}</button>
  {/if}
</div>

<style>
  .queue { height: 100%; display: flex; flex-direction: column; gap: 10px; padding: 12px 16px 16px; overflow-y: auto; }
  .empty { margin: 0; color: var(--muted); font-size: 13px; }
  ol { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: minmax(0, 1fr); gap: 8px; }
  li { display: grid; grid-template-columns: minmax(0, 1fr); gap: 6px; padding: 8px 10px; border-radius: 8px; background: var(--raised); border: 1px solid var(--line); }
  /* Not .row: the app-wide name-value row (app.css) would keep the name on one line. */
  .line { display: flex; align-items: center; gap: 8px; }
  .grip { cursor: grab; color: var(--muted); padding: 0 2px; touch-action: none; }
  .what { flex: 1; min-width: 0; display: grid; }
  .what b, .what small { overflow-wrap: anywhere; }
  .what small { color: var(--muted); font-size: 11px; }
  .st { flex: 1; min-width: 0; font-size: 12px; overflow-wrap: anywhere; }
  .failed .st { color: var(--warn); }
  .done .st { color: var(--muted); }
  .cancelled { opacity: 0.75; }
  .x { border: 0; background: none; padding: 0 4px; font-size: 16px; line-height: 1; color: var(--muted); }
  .bar { height: 3px; border-radius: 2px; background: var(--hover); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--accent); transition: width 0.3s; }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; }
  .acts button, .copy { font-size: 12px; padding: 2px 8px; }
  .log { position: relative; }
  .copy { position: absolute; top: 4px; right: 4px; }
  pre {
    margin: 0;
    max-height: 240px;
    overflow: auto;
    padding: 8px;
    border-radius: 6px;
    background: var(--win);
    font: 11px/1.4 ui-monospace, Menlo, monospace;
    white-space: pre-wrap;
    word-break: break-all;
    user-select: text;
  }
  .clear { align-self: start; }
</style>
