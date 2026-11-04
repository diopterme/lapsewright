<script lang="ts" generics="T">
  type Option = { value: T; label: string; disabled?: boolean; title?: string };

  let { options, value, label, onpick }: { options: Option[]; value: T; label: string; onpick: (value: T) => void } = $props();
</script>

<!-- A mac segmented control: one of a few choices, all visible. -->
<div class="seg" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <button
      role="radio"
      aria-checked={o.value === value}
      class:on={o.value === value}
      disabled={o.disabled}
      title={o.title}
      onclick={() => o.value !== value && onpick(o.value)}>{o.label}</button
    >
  {/each}
</div>

<style>
  .seg { display: flex; padding: 2px; gap: 2px; border-radius: 8px; background: var(--hover); }
  button { flex: 1; border: 0; border-radius: 6px; padding: 3px 9px; background: none; font-size: 13px; white-space: nowrap; }
  button:hover:not(:disabled):not(.on) { background: var(--hover); }
  /* Y5: the picked one in the accent color. */
  .on, .on:hover { background: var(--accent); color: #fff; box-shadow: 0 1px 2px rgb(0 0 0 / 0.18); font-weight: 600; }
</style>
