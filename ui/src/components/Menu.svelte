<script lang="ts">
  // A short menu where the hand is: what can be done with the thing just
  // pressed. It closes on a choice, a touch anywhere else, or Escape.
  import { onMount } from "svelte";

  export type MenuItem = { label: string; pick: () => void; disabled?: boolean; danger?: boolean };
  let { x, y, title = "", items, onclose }: { x: number; y: number; title?: string; items: MenuItem[]; onclose: () => void } = $props();

  let box = $state<HTMLDivElement | null>(null);
  let left = $state(0);
  let top = $state(0);
  let placed = $state(false);

  onMount(() => {
    // inside the window, on the side of the press that has room
    const r = box!.getBoundingClientRect();
    left = Math.max(8, Math.min(x, innerWidth - r.width - 8));
    top = y + r.height + 8 > innerHeight ? Math.max(8, y - r.height) : y;
    placed = true;
    (box!.querySelector("button:not(:disabled)") as HTMLElement | null)?.focus();
    const away = (e: Event) => {
      if (!box?.contains(e.target as Node)) onclose();
    };
    // the press that opened it is still down: listen from the next one
    const t = setTimeout(() => addEventListener("pointerdown", away, true), 0);
    addEventListener("resize", onclose);
    // Escape puts it away wherever the focus happens to be
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      onclose();
    };
    addEventListener("keydown", esc, true);
    return () => {
      removeEventListener("keydown", esc, true);
      clearTimeout(t);
      removeEventListener("pointerdown", away, true);
      removeEventListener("resize", onclose);
    };
  });

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const all = [...box!.querySelectorAll<HTMLElement>("button:not(:disabled)")];
    const i = all.indexOf(document.activeElement as HTMLElement);
    all[(i + (e.key === "ArrowDown" ? 1 : all.length - 1)) % all.length]?.focus();
  }
</script>

<div
  class="menu"
  class:placed
  bind:this={box}
  role="menu"
  aria-label={title || "actions"}
  tabindex="-1"
  style="left: {left}px; top: {top}px"
  onkeydown={key}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#if title}<div class="title">{title}</div>{/if}
  {#each items as item}
    <button
      role="menuitem"
      class:danger={item.danger}
      disabled={item.disabled}
      onclick={() => {
        onclose();
        item.pick();
      }}>{item.label}</button
    >
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 168px;
    max-width: calc(100vw - 16px);
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--edge);
    visibility: hidden;
  }
  .menu.placed {
    visibility: visible;
  }
  .title {
    padding: 8px 12px 6px;
    font: var(--tick);
    color: var(--fg-muted);
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  button {
    border: 0;
    height: 36px;
    padding: 0 12px;
    text-align: left;
  }
  button + button {
    border-top: 1px solid var(--line);
  }
  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--hover);
  }
  button:disabled {
    color: var(--fg-faint);
    cursor: default;
  }
  /* what cannot be put back by hand says so with the one red light */
  button.danger::before {
    content: "";
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-right: 8px;
    background: var(--rd);
    vertical-align: 1px;
  }
  @media (pointer: coarse) {
    button {
      height: 44px;
    }
  }
</style>
