<script setup lang="ts">
import { ref, onMounted } from 'vue'
import ScraperView from './views/ScraperView.vue'
import SettingsView from './views/SettingsView.vue'
import DownloadView from './views/DownloadView.vue'
import LocalView from './views/LocalView.vue'
import SnifferView from './views/SnifferView.vue'

type Tab = 'scraper' | 'download' | 'local' | 'sniffer' | 'settings'

const currentTab = ref<Tab>('local')

// 深色模式
const isDark = ref(false)

onMounted(() => {
  // 从 localStorage 恢复主题
  const saved = localStorage.getItem('theme')
  if (saved === 'dark' || (!saved && window.matchMedia('(prefers-color-scheme: dark)').matches)) {
    isDark.value = true
    document.documentElement.classList.add('dark')
  }
})

function toggleDark() {
  isDark.value = !isDark.value
  if (isDark.value) {
    document.documentElement.classList.add('dark')
    localStorage.setItem('theme', 'dark')
  } else {
    document.documentElement.classList.remove('dark')
    localStorage.setItem('theme', 'light')
  }
}

// 导航标签配置
const tabs: { key: Tab; label: string }[] = [
  { key: 'scraper', label: '爬取视频' },
  { key: 'download', label: '下载视频' },
  { key: 'local', label: '本地管理' },
  { key: 'sniffer', label: '视频嗅探' },
]
</script>

<template>
  <div class="h-screen flex flex-col" :class="isDark ? 'bg-[#0f172a]' : 'bg-gradient-to-br from-[#667eea]/[0.03] to-[#764ba2]/[0.03]'">
    <!-- 顶部导航 -->
    <nav :class="['flex justify-between items-center px-6 h-14 backdrop-blur-sm border-b shrink-0', isDark ? 'bg-[#1e293b]/95 border-gray-700' : 'bg-white/95 border-[#667eea]/10']">
      <div class="flex gap-1">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          :class="[
            'px-5 py-2 rounded-lg text-sm font-medium cursor-pointer transition-all border-none',
            currentTab === tab.key
              ? 'gradient-primary text-white shadow-[0_2px_8px_rgba(102,126,234,0.3)]'
              : isDark
                ? 'bg-transparent text-gray-400 hover:bg-[#667eea]/[0.12] hover:text-[#93a5f5]'
                : 'bg-transparent text-slate-500 hover:bg-[#667eea]/[0.08] hover:text-[#667eea]'
          ]"
          @click="currentTab = tab.key"
        >
          {{ tab.label }}
        </button>
      </div>

      <!-- 右侧按钮组 -->
      <div class="flex items-center gap-1">
        <!-- 深色模式切换 -->
        <button
          :class="['flex items-center justify-center w-9 h-9 bg-transparent border-none rounded-lg cursor-pointer transition-all active:scale-95', isDark ? 'text-amber-400 hover:bg-amber-400/10' : 'text-slate-500 hover:bg-[#667eea]/10 hover:text-[#667eea]']"
          @click="toggleDark"
          :title="isDark ? '切换亮色模式' : '切换深色模式'"
        >
          <!-- 太阳图标 (深色模式下显示) -->
          <svg v-if="isDark" xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="5" />
            <line x1="12" y1="1" x2="12" y2="3" />
            <line x1="12" y1="21" x2="12" y2="23" />
            <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
            <line x1="1" y1="12" x2="3" y2="12" />
            <line x1="21" y1="12" x2="23" y2="12" />
            <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
            <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
          </svg>
          <!-- 月亮图标 (亮色模式下显示) -->
          <svg v-else xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
          </svg>
        </button>

        <!-- 设置按钮 -->
        <button
          :class="['flex items-center justify-center w-9 h-9 bg-transparent border-none rounded-lg cursor-pointer transition-all active:scale-95', isDark ? 'text-gray-400 hover:bg-gray-700 hover:text-gray-300' : 'text-slate-500 hover:bg-[#667eea]/10 hover:text-[#667eea]']"
          @click="currentTab = 'settings'"
          title="设置"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3"></circle>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
          </svg>
        </button>
      </div>
    </nav>

    <!-- 主内容区 -->
    <main class="flex-1 overflow-hidden p-5">
      <ScraperView v-if="currentTab === 'scraper'" />
      <DownloadView v-if="currentTab === 'download'" />
      <LocalView v-if="currentTab === 'local'" />
      <SnifferView v-if="currentTab === 'sniffer'" />
      <SettingsView v-if="currentTab === 'settings'" />
    </main>
  </div>
</template>

<style>
/* Global styles are now in src/styles.css */
</style>

<style scoped>
/* All styles converted to Tailwind CSS */
</style>
