<script setup lang="ts">
import { computed, ref, watch } from "vue";

const props = defineProps<{
  src?: string | null;
  name?: string | null;
  size?: number;
}>();

const initial = computed(() => {
  const n = (props.name || "?").trim();
  return n ? n[0].toUpperCase() : "?";
});

// --- avatar caching ---------------------------------------------------------
// Remote (http/https) avatars are cached as data URLs in localStorage so the
// app works offline and avoids re-downloading the same image on every render.
const displaySrc = ref<string | null>(null);

function cacheKey(src: string): string {
  return "avatar:" + src;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("load failed"));
    img.src = src;
  });
}

function imageToDataUrl(img: HTMLImageElement, size: number): string {
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  const min = Math.min(img.naturalWidth, img.naturalHeight) || size;
  const sx = (img.naturalWidth - min) / 2;
  const sy = (img.naturalHeight - min) / 2;
  ctx.drawImage(img, sx, sy, min, min, 0, 0, size, size);
  return canvas.toDataURL("image/jpeg", 0.8);
}

watch(
  () => props.src,
  async (src) => {
    if (!src) {
      displaySrc.value = null;
      return;
    }
    // Only cache remote URLs; data: URLs are already inline, and tiny
    // relative paths (e.g. file paths) are served locally anyway.
    if (!/^https?:\/\//.test(src)) {
      displaySrc.value = src;
      return;
    }
    const cached = localStorage.getItem(cacheKey(src));
    if (cached) {
      displaySrc.value = cached;
      return;
    }
    displaySrc.value = src; // show the remote URL while we fetch it
    try {
      const img = await loadImage(src);
      const dataUrl = imageToDataUrl(img, 256);
      if (dataUrl.length < 400_000) {
        // Cap the cache size: drop the oldest entry if we exceed 100 entries.
        localStorage.setItem(cacheKey(src), dataUrl);
        if (localStorage.length > 100 + 30) {
          for (let i = 0; i < localStorage.length; i++) {
            const key = localStorage.key(i);
            if (key && key.startsWith("avatar:")) {
              localStorage.removeItem(key);
              break;
            }
          }
        }
        displaySrc.value = dataUrl;
      }
    } catch {
      // CORS or network failure: keep showing the remote URL.
    }
  },
  { immediate: true },
);
</script>

<template>
  <span
    class="avatar"
    :style="{ width: (size || 36) + 'px', height: (size || 36) + 'px', fontSize: ((size || 36) * 0.42) + 'px' }"
  >
    <img v-if="displaySrc" :src="displaySrc" alt="" />
    <span v-else class="placeholder">{{ initial }}</span>
  </span>
</template>

<style scoped>
.avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  overflow: hidden;
  background: var(--bg-3);
  border: 1px solid var(--border);
  flex-shrink: 0;
  vertical-align: middle;
}
.avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.placeholder {
  color: var(--text-dim);
  font-weight: 700;
}
</style>
