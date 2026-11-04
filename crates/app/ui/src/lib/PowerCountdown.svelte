<script lang="ts">
  import { queue } from "./queue.svelte.ts";
  import { t } from "./i18n";

  const WORDS = { none: "", sleep: t("잠자기", "sleep"), shutdown: t("시스템 종료", "shut down") };
  const NOW = { none: "", sleep: t("지금 잠자기", "Sleep Now"), shutdown: t("지금 시스템 종료", "Shut Down Now") };
</script>

<!-- F5: the last chance before the Mac sleeps or shuts down. -->
{#if queue.countdown}
  {@const word = WORDS[queue.countdown.power]}
  {@const left = queue.countdown.left}
  <div class="veil">
    <div class="box" role="alertdialog" aria-labelledby="power-title" aria-describedby="power-why">
      <p id="power-title">{t("", `The Mac will ${word} in `)}<b class="num">{t(`${left}초`, `${left} s`)}</b>{t(` 뒤 맥 ${word}`, "")}</p>
      <p id="power-why" class="why">{t("대기열이 다 끝났음. 설정 창 › 끝났을 때 에서 고른 것.", "The queue is done. Chosen in Settings › When Done.")}</p>
      <div class="buttons">
        <button onclick={() => queue.stopCountdown()}>{t("취소", "Cancel")}</button>
        <button class="primary" onclick={() => queue.powerNow()}>{NOW[queue.countdown.power]}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; display: grid; place-items: center; background: rgb(0 0 0 / 0.35); }
  .box { display: grid; gap: 8px; min-width: 320px; padding: 20px; border-radius: calc(var(--radius) * 1.5); background: var(--raised); box-shadow: 0 10px 40px rgb(0 0 0 / 0.3); }
  p { margin: 0; }
  #power-title { font-size: 17px; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px; }
</style>
