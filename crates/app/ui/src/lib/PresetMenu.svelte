<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { deletePreset, presets, restoreBuiltIns, savePreset, type PresetEntry, type Session } from "./api";
  import { sameSettings } from "./settings";
  import { t } from "./i18n";

  let { session, onchange }: { session: Session; onchange: (next: Session) => Promise<Session> } = $props();

  /** Menu values that are actions, not presets (R3, R4, R6). Preset ids never start with `@`. */
  const NEW = "@new";
  const SAVE = "@save";
  const REVERT = "@revert";
  const RENAME = "@rename";
  const DELETE = "@delete";
  const RESTORE = "@restore";

  let list = $state<PresetEntry[]>([]);
  let error = $state<string | null>(null);
  /** The name dialog (R3) and, for renaming, which preset. */
  let naming = $state<{ renaming: PresetEntry | null; name: string; problem: string | null } | null>(null);

  const current = $derived(list.find((p) => p.id === session.preset && p.settings));
  /** R2: values changed since the preset was picked or saved. */
  const isChanged = $derived(!!current?.settings && !sameSettings(current.settings, session.settings));

  onMount(() => void run(reload));

  async function reload() {
    list = await presets();
  }

  /** Errors show under the menu; the panel keeps working. */
  async function run(job: () => Promise<unknown>) {
    try {
      await job();
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  function onmenu(e: Event & { currentTarget: HTMLSelectElement }) {
    const value = e.currentTarget.value;
    // An action is not a pick: the menu goes back to the preset in use.
    e.currentTarget.value = session.preset ?? "";
    if (value === NEW) naming = { renaming: null, name: "", problem: null };
    else if (value === SAVE && current) save(current);
    else if (value === REVERT && current?.settings) run(() => onchange({ ...session, settings: current.settings! }));
    else if (value === RENAME && current) naming = { renaming: current, name: current.name, problem: null };
    else if (value === DELETE && current) remove(current);
    else if (value === RESTORE) restore();
    else pick(value);
  }

  function pick(id: string) {
    const settings = list.find((p) => p.id === id)?.settings;
    if (settings) run(() => onchange({ ...session, preset: id, settings }));
  }

  async function submitName() {
    if (!naming) return;
    const name = naming.name.trim();
    const { renaming } = naming;
    const same = list.find((p) => p.name === name && p.id !== renaming?.id);
    if (!name) return void (naming.problem = t("이름을 넣으세요", "Enter a name"));
    if (renaming) {
      if (same) return void (naming.problem = t("같은 이름이 있음", "That name is taken"));
      naming = null;
      return run(async () => {
        await savePreset(renaming.id, name, renaming.settings!);
        await reload();
      });
    }
    naming = null;
    const overwrite = t(`"${name}" 프리셋이 이미 있음. 지금 값으로 덮을까요?`, `A preset named "${name}" already exists. Replace it with the current values?`);
    if (same && !(await ask(overwrite, { title: t("새로 저장", "Save as New"), kind: "warning", okLabel: t("덮기", "Replace"), cancelLabel: t("취소", "Cancel") }))) return;
    await run(async () => {
      const id = await savePreset(same?.id ?? null, name, session.settings);
      await reload();
      await onchange({ ...session, preset: id });
    });
  }

  async function remove(target: PresetEntry) {
    const question = t(`"${target.name}" 프리셋을 지울까요?`, `Delete the preset "${target.name}"?`);
    if (!(await ask(question, { title: t("프리셋 지우기", "Delete Preset"), kind: "warning", okLabel: t("지우기", "Delete"), cancelLabel: t("취소", "Cancel") }))) return;
    // The values stay; only the name they came from is gone.
    await run(async () => {
      await deletePreset(target.id);
      await onchange({ ...session, preset: null });
      await reload();
    });
  }

  /** D6: deleted built-ins come back; changed ones are replaced only after asking. */
  async function restore() {
    await run(async () => {
      const changed = await restoreBuiltIns(false);
      const question = t(`고쳐 쓴 기본 프리셋을 처음 값으로 덮을까요?\n\n${changed.join("\n")}`, `Reset these changed built-in presets to their original values?\n\n${changed.join("\n")}`);
      if (changed.length && (await ask(question, { title: t("기본 프리셋 되살리기", "Restore Built-in Presets"), kind: "warning", okLabel: t("덮기", "Reset"), cancelLabel: t("그대로 두기", "Keep Mine") }))) {
        await restoreBuiltIns(true);
      }
      await reload();
    });
  }

  const save = (target: PresetEntry) =>
    run(async () => {
      await savePreset(target.id, target.name, session.settings);
      await reload();
    });
</script>

<div class="box">
  <div class="row">
    <span>{t("프리셋", "Preset")}</span>
    <span class="value">
      <!-- The dot's place stays so the menu doesn't move. -->
      <span class="dot" class:off={!isChanged} title={isChanged ? t("바꾼 값이 저장 안 됨", "Changes not saved") : undefined}></span>
      <!-- R1: the native menu. -->
      <select value={session.preset ?? ""} onchange={onmenu} aria-label={t("프리셋", "Preset")}>
        {#if !session.preset}<option value="" disabled>{t("프리셋 안 고름", "No Preset")}</option>{/if}
        {#each list as p (p.id)}
          <option value={p.id} disabled={!p.settings} title={p.error ?? undefined}>{p.settings ? p.name : t(`${p.name} (못 읽음)`, `${p.name} (can’t read)`)}</option>
        {/each}
        <option disabled>──────────</option>
        {#if current}
          <option value={SAVE} disabled={!isChanged}>{t(`"${current.name}" 에 지금 값 저장`, `Save Changes to "${current.name}"`)}</option>
          <option value={REVERT} disabled={!isChanged}>{t(`"${current.name}" 값으로 되돌리기`, `Revert to "${current.name}"`)}</option>
        {/if}
        <option value={NEW}>{t("새로 저장…", "Save as New…")}</option>
        {#if current}
          <option value={RENAME}>{t(`"${current.name}" 이름 바꾸기…`, `Rename "${current.name}"…`)}</option>
          <option value={DELETE}>{t(`"${current.name}" 지우기…`, `Delete "${current.name}"…`)}</option>
        {/if}
        <option disabled>──────────</option>
        <option value={RESTORE}>{t("기본 프리셋 되살리기", "Restore Built-in Presets")}</option>
      </select>
    </span>
    {#if error}<p class="cap warn">{error}</p>{/if}
  </div>
</div>

{#if naming}
  <dialog {@attach (el) => el.showModal()} onclose={() => (naming = null)}>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        submitName();
      }}
    >
      <b>{naming.renaming ? t("이름 바꾸기", "Rename") : t("새 프리셋", "New Preset")}</b>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={naming.name} placeholder={t("이름", "Name")} autofocus aria-label={t("프리셋 이름", "Preset name")} />
      {#if naming.problem}<p class="why warn">{naming.problem}</p>{/if}
      <div class="buttons">
        <button type="button" onclick={() => (naming = null)}>{t("취소", "Cancel")}</button>
        <button class="primary" type="submit">{naming.renaming ? t("바꾸기", "Rename") : t("저장", "Save")}</button>
      </div>
    </form>
  </dialog>
{/if}

<style>
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); flex: none; }
  .dot.off { visibility: hidden; }
  .value { flex: 1; justify-content: flex-end; }
  select { min-width: 0; }
  dialog { border: 1px solid var(--line); border-radius: var(--radius); background: var(--raised); color: var(--text); padding: 18px; width: 300px; box-shadow: 0 12px 40px rgb(0 0 0 / 0.3); }
  dialog::backdrop { background: rgb(0 0 0 / 0.2); }
  form { display: grid; gap: 10px; }
  input { font: inherit; padding: 5px 8px; border-radius: 6px; border: 1px solid var(--line); background: var(--win); color: var(--text); user-select: text; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; }
</style>
