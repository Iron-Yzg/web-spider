<script setup lang="ts">
import { ref } from 'vue'
import ScraperView from './views/ScraperView.vue'
import SettingsView from './views/SettingsView.vue'
import DownloadView from './views/DownloadView.vue'
import LocalView from './views/LocalView.vue'

type Tab = 'scraper' | 'download' | 'local' | 'settings'

const currentTab = ref<Tab>('local')
</script>

<template>
  <div class="h-screen flex flex-col bg-gradient-to-br from-[#667eea]/[0.03] to-[#764ba2]/[0.03]">
    <!-- 顶部导航 -->
    <nav class="flex justify-between items-center px-6 h-14 bg-white/95 backdrop-blur-sm border-b border-[#667eea]/10 shrink-0">
      <div class="flex gap-1">
        <button
          :class="[
            'px-5 py-2 rounded-lg text-sm font-medium cursor-pointer transition-all border-none',
            currentTab === 'scraper' 
              ? 'gradient-primary text-white shadow-[0_2px_8px_rgba(102,126,234,0.3)]' 
              : 'bg-transparent text-slate-500 hover:bg-[#667eea]/[0.08] hover:text-[#667eea]'
          ]"
          @click="currentTab = 'scraper'"
        >
          爬取视频
        </button>
        <button
          :class="[
            'px-5 py-2 rounded-lg text-sm font-medium cursor-pointer transition-all border-none',
            currentTab === 'download' 
              ? 'gradient-primary text-white shadow-[0_2px_8px_rgba(102,126,234,0.3)]' 
              : 'bg-transparent text-slate-500 hover:bg-[#667eea]/[0.08] hover:text-[#667eea]'
          ]"
          @click="currentTab = 'download'"
        >
          下载视频
        </button>
        <button
          :class="[
            'px-5 py-2 rounded-lg text-sm font-medium cursor-pointer transition-all border-none',
            currentTab === 'local' 
              ? 'gradient-primary text-white shadow-[0_2px_8px_rgba(102,126,234,0.3)]' 
              : 'bg-transparent text-slate-500 hover:bg-[#667eea]/[0.08] hover:text-[#667eea]'
          ]"
          @click="currentTab = 'local'"
        >
          本地管理
        </button>
      </div>

      <!-- 右侧设置按钮 -->
      <button 
        class="flex items-center justify-center w-9 h-9 bg-transparent border-none rounded-lg text-slate-500 cursor-pointer transition-all hover:bg-[#667eea]/10 hover:text-[#667eea] active:scale-95"
        @click="currentTab = 'settings'" 
        title="设置"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"></circle>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
        </svg>
      </button>
    </nav>

    <!-- 主内容区 -->
    <main class="flex-1 overflow-hidden p-5">
      <ScraperView v-if="currentTab === 'scraper'" />
      <DownloadView v-if="currentTab === 'download'" />
      <LocalView v-if="currentTab === 'local'" />
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
