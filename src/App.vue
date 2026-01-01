<script setup lang="ts">
import { ref } from 'vue'
import ConfigPage from './components/ConfigPage.vue'
import ScraperPage from './components/ScraperPage.vue'

type Tab = 'scraper' | 'config'

const currentTab = ref<Tab>('scraper')
</script>

<template>
  <div class="app">
    <!-- 顶部导航 -->
    <nav class="nav-bar">
      <div class="nav-brand">🎬 M3U8 爬虫</div>
      <div class="nav-tabs">
        <button
          :class="['nav-tab', { active: currentTab === 'scraper' }]"
          @click="currentTab = 'scraper'"
        >
          爬取视频
        </button>
        <button
          :class="['nav-tab', { active: currentTab === 'config' }]"
          @click="currentTab = 'config'"
        >
          设置
        </button>
      </div>
    </nav>

    <!-- 主内容区 -->
    <main class="main-content">
      <ScraperPage v-if="currentTab === 'scraper'" />
      <ConfigPage v-if="currentTab === 'config'" />
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

/* 主内容区 */
.main-content {
  flex: 1;
  overflow: hidden;
  padding: 20px;
}
</style>
