<script lang="ts">
  import { CRF_STEPS, MAX_CRF } from "./settings";
  import { t } from "./i18n";

  let { crf, disabled, onpick }: { crf: number; disabled: boolean; onpick: (crf: number) => void } = $props();

  /** The slider stops here, where the video is already blocky; the number box takes up to MAX_CRF.
   *  A shorter slider also keeps the step names from overlapping on a narrow panel. */
  const SLIDER_WORST = 40;
  const THUMB = 16;

  /** While dragging: the value under the thumb, applied on release. */
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? crf);
  /** C7: reversed, so the right end is the best quality. */
  const at = (c: number) => (SLIDER_WORST - Math.min(c, SLIDER_WORST)) / SLIDER_WORST;
  const fromSlider = (e: Event) => SLIDER_WORST - Number((e.currentTarget as HTMLInputElement).value);

  function typed(e: Event & { currentTarget: HTMLInputElement }) {
    const n = Number(e.currentTarget.value);
    if (Number.isInteger(n) && n >= 0 && n <= MAX_CRF) onpick(n);
    else e.currentTarget.value = String(crf);
  }
</script>

<div class="quality" class:disabled style:--thumb="{THUMB}px">
  <div class="head">
    <span>{t("화질", "Quality")}</span>
    <input class="num" type="number" min="0" max={MAX_CRF} value={shown} {disabled} onchange={typed} aria-label={t("화질 숫자 (작을수록 좋음)", "Quality number (lower is better)")} />
  </div>
  <input
    type="range"
    min="0"
    max={SLIDER_WORST}
    value={SLIDER_WORST * at(shown)}
    style:--fill={at(shown)}
    {disabled}
    oninput={(e) => (dragging = fromSlider(e))}
    onchange={(e) => {
      dragging = null;
      onpick(fromSlider(e));
    }}
    aria-label={t("화질 (오른쪽이 좋은 화질)", "Quality (better to the right)")}
  />
  <div class="ticks">
    {#each CRF_STEPS as step (step.crf)}
      <button class:on={shown === step.crf} style:--at={at(step.crf)} {disabled} title={t(`화질 ${step.crf}`, `Quality ${step.crf}`)} onclick={() => onpick(step.crf)}
        >{step.label}</button
      >
    {/each}
  </div>
  {#if disabled}<p class="why">{t("ProRes 는 프로파일로 화질이 정해짐", "ProRes quality is set by the profile")}</p>{/if}
</div>

<style>
  .quality { display: grid; gap: 2px; }
  .head { display: flex; justify-content: space-between; align-items: center; }
  .head input { width: 6ch; text-align: right; }
  .disabled .head span, .disabled .ticks button { color: var(--faint); }
  input[type="range"] { -webkit-appearance: none; appearance: none; width: 100%; height: 20px; margin: 0; background: transparent; }
  input[type="range"]::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) calc(var(--fill) * 100%), var(--line) 0);
  }
  input[type="range"]:disabled::-webkit-slider-runnable-track { background: var(--line); }
  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: var(--thumb);
    height: var(--thumb);
    margin-top: calc((4px - var(--thumb)) / 2);
    border-radius: 50%;
    background: var(--raised);
    border: 1px solid var(--line);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.25);
  }
  /* Centered under the thumb's position for that value. */
  .ticks { position: relative; height: 20px; }
  .ticks button {
    position: absolute;
    left: calc(var(--thumb) / 2 + var(--at) * (100% - var(--thumb)));
    transform: translateX(-50%);
    border: 0;
    background: none;
    padding: 0 2px;
    font-size: 11px;
    color: var(--muted);
  }
  .ticks button:hover:not(:disabled) { color: var(--text); background: none; }
  .ticks .on { color: var(--accent); font-weight: 600; }
</style>
