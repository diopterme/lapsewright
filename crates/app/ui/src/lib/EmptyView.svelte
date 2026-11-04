<script lang="ts">
  import { onDestroy } from "svelte";
  import { recentThumbUrl, type Recent } from "./api";
  import { count } from "./format";
  import { t } from "./i18n";

  let { recent, error, onpick, onopen }: {
    recent: Recent[];
    error: string | null;
    onpick: () => void;
    onopen: (path: string) => void;
  } = $props();

  /** Y10: card pictures by folder path. */
  let thumbs = $state<Record<string, string>>({});
  const asked = new Set<string>();

  $effect(() => {
    for (const folder of recent.filter((f) => f.exists && !asked.has(f.path))) {
      asked.add(folder.path);
      // A card without a picture shows the folder icon; nothing else depends on it.
      recentThumbUrl(folder.path).then((url) => (thumbs = { ...thumbs, [folder.path]: url }), () => {});
    }
  });

  onDestroy(() => Object.values(thumbs).forEach((url) => URL.revokeObjectURL(url)));
</script>

<div class="empty">
  <div class="hero">
    <svg class="art" viewBox="0 0 120 90" aria-hidden="true">
      <rect x="18" y="14" width="70" height="52" rx="6" transform="rotate(-8 53 40)" />
      <rect x="30" y="20" width="70" height="52" rx="6" transform="rotate(5 65 46)" />
      <rect x="24" y="26" width="72" height="54" rx="6" />
      <circle cx="76" cy="40" r="6" />
      <path d="M28 74l20-22 14 14 9-8 21 16" />
    </svg>
    <h1>{t("사진 폴더를 끌어다 놓으세요", "Drop a Photo Folder Here")}</h1>
    <p class="hint">{t("JPG · PNG · JXL · TIF 사진이 든 폴더. 창 아무 데나 놓아도 됨.", "A folder of JPG · PNG · JXL · TIF photos. Drop it anywhere in the window.")}</p>
    <button class="primary" onclick={onpick}>{t("폴더 열기…", "Open Folder…")}</button>
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  {#if recent.length}
    <section>
      <h2>{t("최근 폴더", "Recent Folders")}</h2>
      <ul>
        {#each recent as folder (folder.path)}
          <li>
            <button class="card" disabled={!folder.exists} title={folder.exists ? folder.path : t(`없음: ${folder.path}`, `Missing: ${folder.path}`)} onclick={() => onopen(folder.path)}>
              {#if thumbs[folder.path]}
                <img src={thumbs[folder.path]} alt="" draggable="false" />
              {:else}
                <span class="pic">
                  <svg viewBox="0 0 20 16" aria-hidden="true">
                    <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h4.6l1.6 1.8h8.8A1.5 1.5 0 0 1 19 5.3v8.2a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 1 13.5z" />
                  </svg>
                </span>
              {/if}
              <b>{folder.name}</b>
              <span class="num">{folder.exists ? t(`${count(folder.count)}장`, `${count(folder.count)} photos`) : t("없음", "Missing")}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .empty { height: 100%; display: grid; align-content: center; justify-items: center; gap: 36px; padding: 24px 32px; overflow-y: auto; }
  .hero { display: grid; justify-items: center; gap: 8px; text-align: center; }
  .art { width: 120px; margin-bottom: 6px; fill: var(--raised); stroke: var(--muted); stroke-width: 2; }
  .art circle, .art path { fill: none; }
  h1 { margin: 0; font-size: 20px; }
  p { margin: 0; }
  .hint { color: var(--muted); font-size: 13px; }
  .hero button { margin-top: 8px; padding: 6px 22px; border-radius: 999px; font-weight: 600; }
  .error { color: var(--warn); max-width: 46ch; }
  section { width: min(640px, 100%); display: grid; gap: 8px; }
  h2 { margin: 0 0 0 2px; font-size: 12px; font-weight: 700; color: var(--muted); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 14px; }
  .card { width: 100%; display: grid; gap: 4px; border: 0; padding: 0; background: none; text-align: left; }
  .card:hover:not(:disabled) { background: none; }
  .card:hover:not(:disabled) img, .card:hover:not(:disabled) .pic { outline: 2px solid var(--accent); }
  img, .pic { width: 100%; aspect-ratio: 3 / 2; object-fit: cover; border-radius: 6px; box-shadow: 0 0 0 1px var(--line); }
  .pic { display: grid; place-items: center; background: var(--hover); }
  .pic svg { width: 32px; fill: var(--faint); }
  b { font-size: 12px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .card span.num { font-size: 11px; color: var(--muted); }
  .card:disabled { opacity: 0.5; }
</style>
