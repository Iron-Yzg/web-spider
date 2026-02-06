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
  <div class="app">
    <!-- 顶部导航 -->
    <nav class="nav-bar">
      <div class="nav-tabs">
        <button
          :class="['nav-tab', { active: currentTab === 'scraper' }]"
          @click="currentTab = 'scraper'"
        >
          爬取视频
        </button>
        <button
          :class="['nav-tab', { active: currentTab === 'download' }]"
          @click="currentTab = 'download'"
        >
          下载视频
        </button>
        <button
          :class="['nav-tab', { active: currentTab === 'local' }]"
          @click="currentTab = 'local'"
        >
          本地管理
        </button>
      </div>

      <!-- 右侧设置按钮 -->
      <button class="settings-btn" @click="currentTab = 'settings'" title="设置">
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"></circle>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
        </svg>
      </button>
    </nav>

    <!-- 主内容区 -->
    <main class="main-content">
      <ScraperView v-if="currentTab === 'scraper'" />
      <DownloadView v-if="currentTab === 'download'" />
      <LocalView v-if="currentTab === 'local'" />
      <SettingsView v-if="currentTab === 'settings'" />
    </main>
  </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
  background: #f5f7fa;
  color: #1a1a2e;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  overflow: hidden;
  height: 100vh;
}

#app {
  height: 100vh;
  overflow: hidden;
}
</style>

<style scoped>
.app {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: linear-gradient(135deg, #667eea08 0%, #764ba208 100%);
}

/* 导航栏 */
.nav-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 24px;
  height: 56px;
  background: rgba(255, 255, 255, 0.95);
  backdrop-filter: blur(10px);
  border-bottom: 1px solid rgba(102, 126, 234, 0.1);
  flex-shrink: 0;
}

.nav-brand {
  font-size: 18px;
  font-weight: 700;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.nav-tabs {
  display: flex;
  gap: 4px;
}

.nav-tab {
  padding: 8px 20px;
  background: transparent;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s;
}

.nav-tab:hover {
  background: rgba(102, 126, 234, 0.08);
  color: #667eea;
}

.nav-tab.active {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  box-shadow: 0 2px 8px rgba(102, 126, 234, 0.3);
}

/* 设置按钮 */
.settings-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  background: transparent;
  border: none;
  border-radius: 8px;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s;
}

.settings-btn:hover {
  background: rgba(102, 126, 234, 0.1);
  color: #667eea;
}

.settings-btn:active {
  transform: scale(0.95);
}

/* 主内容区 */
.main-content {
  flex: 1;
  overflow: hidden;
  padding: 20px;
}
</style>
