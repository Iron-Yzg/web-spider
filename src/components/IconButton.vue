<script setup lang="ts">
defineProps<{
  variant?: 'play' | 'pause' | 'stop' | 'start' | 'cast' | 'download' | 'delete' | 'folder' | 'add' | 'default'
  size?: 'sm' | 'md'
  disabled?: boolean
}>()

defineEmits<{
  (e: 'click', event: MouseEvent): void
}>()
</script>

<template>
  <button
    type="button"
    class="icon-btn"
    :class="[
      `icon-btn--${variant || 'default'}`,
      { 'icon-btn--sm': size === 'sm' }
    ]"
    :disabled="disabled"
    @click="$emit('click', $event)"
  >
    <!-- Play -->
    <svg v-if="variant === 'play'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true">
      <polygon points="8 5 19 12 8 19 8 5"></polygon>
    </svg>
    <!-- Pause -->
    <svg v-else-if="variant === 'pause'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <line x1="10" y1="5" x2="10" y2="19"></line>
      <line x1="14" y1="5" x2="14" y2="19"></line>
    </svg>
    <!-- Stop -->
    <svg v-else-if="variant === 'stop'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true">
      <rect x="7" y="7" width="10" height="10" rx="1"></rect>
    </svg>
    <!-- Start -->
    <svg v-else-if="variant === 'start'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true">
      <polygon points="8 5 19 12 8 19 8 5"></polygon>
    </svg>
    <!-- Cast (DLNA) -->
    <svg v-else-if="variant === 'cast'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M2 17a5 5 0 0 1 5 5"></path>
      <path d="M2 13a9 9 0 0 1 9 9"></path>
      <rect x="3" y="3" width="18" height="13" rx="2"></rect>
    </svg>
    <!-- Download -->
    <svg v-else-if="variant === 'download'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M12 3v11"></path>
      <path d="m7.5 10.5 4.5 4.5 4.5-4.5"></path>
      <path d="M4 19h16"></path>
    </svg>
    <!-- Delete -->
    <svg v-else-if="variant === 'delete'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M3 6h18"></path>
      <path d="M8 6V4h8v2"></path>
      <path d="M19 6l-1 14H6L5 6"></path>
      <line x1="10" y1="10" x2="10" y2="17"></line>
      <line x1="14" y1="10" x2="14" y2="17"></line>
    </svg>
    <!-- Folder -->
    <svg v-else-if="variant === 'folder'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M3 6a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v1H3V6z"></path>
      <path d="M3 10h18l-1.2 8.5a2 2 0 0 1-2 1.5H6.2a2 2 0 0 1-2-1.5L3 10z"></path>
    </svg>
    <!-- Add -->
    <svg v-else-if="variant === 'add'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <line x1="12" y1="5" x2="12" y2="19"></line>
      <line x1="5" y1="12" x2="19" y2="12"></line>
    </svg>
    <!-- Default (no icon) -->
    <slot v-else />
  </button>
</template>

<style scoped>
.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  background: white;
  color: #64748b;
  cursor: pointer;
  transition: all 0.16s;
}

.icon-btn:hover:not(:disabled) {
  background: #f8fafc;
  border-color: #cbd5e1;
}

.icon-btn:active:not(:disabled) {
  transform: scale(0.95);
}

.icon-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.icon-btn--sm {
  width: 26px;
  height: 26px;
}

.icon-btn--sm svg {
  width: 12px;
  height: 12px;
}

/* Play - 蓝色 */
.icon-btn--play {
  background: #eef6ff;
  border-color: #93c5fd;
  color: #2563eb;
}

.icon-btn--play:hover:not(:disabled) {
  background: #dbeafe;
  border-color: #60a5fa;
}

/* Pause - 琥珀色 */
.icon-btn--pause {
  background: #fff7ed;
  border-color: #fdba74;
  color: #ea580c;
}

.icon-btn--pause:hover:not(:disabled) {
  background: #ffedd5;
  border-color: #fb923c;
}

/* Stop - 红色 */
.icon-btn--stop {
  background: #fff1f2;
  border-color: #fda4af;
  color: #e11d48;
}

.icon-btn--stop:hover:not(:disabled) {
  background: #ffe4e6;
  border-color: #fb7185;
}

/* Start - 绿色 */
.icon-btn--start {
  background: #f0fdf4;
  border-color: #86efac;
  color: #16a34a;
}

.icon-btn--start:hover:not(:disabled) {
  background: #dcfce7;
  border-color: #4ade80;
}

/* Cast - 紫色 */
.icon-btn--cast {
  background: #eef2ff;
  border-color: #a5b4fc;
  color: #4f46e5;
}

.icon-btn--cast:hover:not(:disabled) {
  background: #e0e7ff;
  border-color: #818cf8;
}

/* Download - 橙色 */
.icon-btn--download {
  background: #ecfeff;
  border-color: #67e8f9;
  color: #0891b2;
}

.icon-btn--download:hover:not(:disabled) {
  background: #cffafe;
  border-color: #22d3ee;
}

/* Delete - 红色 */
.icon-btn--delete {
  background: #fff1f2;
  border-color: #fda4af;
  color: #e11d48;
}

.icon-btn--delete:hover:not(:disabled) {
  background: #ffe4e6;
  border-color: #fb7185;
}

/* Folder - 灰色 */
.icon-btn--folder {
  background: #f8fafc;
  border-color: #cbd5e1;
  color: #475569;
}

.icon-btn--folder:hover:not(:disabled) {
  background: #f1f5f9;
  border-color: #94a3b8;
}

/* Add - 绿色 */
.icon-btn--add {
  background: #f0fdf4;
  border-color: #86efac;
  color: #16a34a;
}

.icon-btn--add:hover:not(:disabled) {
  background: #dcfce7;
  border-color: #4ade80;
}
</style>
