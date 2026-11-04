<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { outputProblem, type Folder } from "./api";
  import { t } from "./i18n";

  let { output, folder, onpick }: { output: string | null; folder: Folder | null; onpick: (dir: string | null) => void } = $props();

  /** C10: next to the photo folder until another folder is picked. */
  const dir = $derived(output ?? folder?.parent ?? null);
  let problem = $state<string | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const checking = dir;
    problem = null;
    if (checking) outputProblem(checking).then((p) => checking === dir && (problem = p), (e) => (problem = String(e)));
  });

  const lastPart = (path: string) => path.split("/").filter(Boolean).pop() ?? path;

  async function change() {
    try {
      const picked = await open({ directory: true, title: t("영상 저장할 폴더", "Folder for the Video"), defaultPath: dir ?? undefined });
      if (typeof picked === "string") onpick(picked);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }
</script>

<!-- A row of the 저장 box (Y4). -->
<div class="row">
  <span>{t("출력", "Output")}</span>
  <span class="value">
    <span class="where" title={dir ?? t("사진 폴더를 열면 정해짐", "Set when a photo folder is open")}>{output ? lastPart(output) : t("사진 폴더 옆", "Next to the photos")}</span>
    <button class="mini" onclick={change}>{t("변경…", "Change…")}</button>
  </span>
  {#if output}
    <p class="cap"><button class="link" onclick={() => onpick(null)}>{t("사진 폴더 옆으로", "Back to next to the photos")}</button></p>
  {/if}
  {#if problem}<p class="cap warn">⚠ {problem}</p>{/if}
  {#if error}<p class="cap warn">{error}</p>{/if}
</div>

<style>
  .where { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
