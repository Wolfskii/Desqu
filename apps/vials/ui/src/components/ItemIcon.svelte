<script module lang="ts">
  // Shared by every icon on the page: extraction costs a few ms each, and the same item shows up
  // in several lists.
  const cache = new Map<string, string>();
</script>

<script lang="ts">
  import { api } from "../api";

  let { id, size = 32, dim = false }: { id: string; size?: number; dim?: boolean } = $props();

  let host: HTMLSpanElement | undefined = $state();
  let visible = $state(false);
  let url = $state<string | null>(null);

  // Only load icons that are on screen: a desktop can have hundreds of items.
  $effect(() => {
    if (!host) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        visible = true;
        observer.disconnect();
      }
    });
    observer.observe(host);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (!visible) return;
    const key = `${id}|${size}`;
    const hit = cache.get(key);
    if (hit) {
      url = hit;
      return;
    }
    url = null;
    let cancelled = false;
    api
      .itemIcon(id, size)
      .then((u) => {
        cache.set(key, u);
        if (!cancelled) url = u;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });
</script>

<span class="icon" class:dim bind:this={host} style="width:{size}px;height:{size}px">
  {#if url}
    <img src={url} alt="" width={size} height={size} draggable="false" />
  {/if}
</span>

<style>
  .icon {
    display: inline-block;
    flex: none;
    border-radius: 6px;
    background: var(--surface-2);
  }
  .icon.dim {
    opacity: 0.45;
  }
  img {
    display: block;
  }
</style>
