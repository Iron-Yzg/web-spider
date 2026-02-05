<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { AppConfig, Website, LocalStorageItem, ScraperInfo, YtdlpConfig } from '../types'

// 设置分类
type SettingSection = 'websites' | 'download' | 'ytdlp' | 'proxy' | 'ai'

interface NavItem {
  id: SettingSection
  label: string
}

// 导航列表（无图标）
const navItems: NavItem[] = [
  { id: 'websites', label: '网站设置' },
  { id: 'download', label: '下载设置' },
  { id: 'ytdlp', label: 'yt-dlp 设置' },
  { id: 'proxy', label: '代理设置' },
  { id: 'ai', label: 'AI 设置' },
]

const activeSection = ref<SettingSection>('websites')

// 下载配置
const config = ref<AppConfig>({
  download_path: './downloads',
  local_storage: [],
  default_quality: 'auto'
})

// yt-dlp 配置
const ytdlpConfig = ref<YtdlpConfig>({
  quality: 'Best' as any,
  format: 'mp4',
  subtitles: false,
  subtitle_langs: 'zh-CN,zh-Hans,zh-Hant,en',
  thumbnail: false,
  audio_only: false,
  audio_format: 'mp3',
  merge_video: true,
  concurrent_downloads: 3,
  extra_options: '',
})

// 网站列表
const websites = ref<Website[]>([])
const isLoading = ref(true)
const isSaving = ref(false)
const saveMessage = ref<{ type: 'success' | 'error', text: string } | null>(null)

// 可用爬虫列表
const scrapers = ref<ScraperInfo[]>([])

// 弹窗状态
const showWebsiteForm = ref(false)
const editingWebsite = ref<Website | null>(null)
const websiteForm = ref({
  name: '',
  base_url: '',
  spider: 'd1',
  local_storage: [] as LocalStorageItem[]
})
const newStorageKey = ref('')
const newStorageValue = ref('')

let saveMessageTimeout: ReturnType<typeof setTimeout> | null = null

onMounted(async () => {
  await Promise.all([
    loadConfig(),
    loadYtdlpConfig(),
    loadWebsites(),
    loadScrapers()
  ])
  isLoading.value = false
})

async function loadConfig() {
  try {
    config.value = await invoke<AppConfig>('get_config')
  } catch (e) {
    console.error('加载配置失败:', e)
  }
}

async function loadYtdlpConfig() {
  try {
    const saved = await invoke<YtdlpConfig>('get_ytdlp_config')
    if (saved) {
      ytdlpConfig.value = saved
    }
  } catch (e) {
    console.error('加载 yt-dlp 配置失败:', e)
  }
}

async function loadWebsites() {
  try {
    websites.value = await invoke<Website[]>('get_websites')
  } catch (e) {
    console.error('加载网站列表失败:', e)
  }
}

async function loadScrapers() {
  try {
    scrapers.value = await invoke<ScraperInfo[]>('get_scrapers')
  } catch (e) {
    console.error('加载爬虫列表失败:', e)
  }
}

function showSaveMessage(type: 'success' | 'error', text: string) {
  if (saveMessageTimeout) {
    clearTimeout(saveMessageTimeout)
  }
  saveMessage.value = { type, text }
  saveMessageTimeout = setTimeout(() => {
    saveMessage.value = null
  }, 3000)
}

// ========== 网站管理 ==========

function openWebsiteForm(website?: Website) {
  if (website) {
    editingWebsite.value = website
    websiteForm.value = {
      name: website.name,
      base_url: website.base_url,
      spider: website.spider || 'd1',
      local_storage: [...website.local_storage]
    }
  } else {
    editingWebsite.value = null
    websiteForm.value = {
      name: '',
      base_url: '',
      spider: 'd1',
      local_storage: []
    }
  }
  newStorageKey.value = ''
  newStorageValue.value = ''
  showWebsiteForm.value = true
}

function closeWebsiteForm() {
  showWebsiteForm.value = false
  editingWebsite.value = null
}

function addStorageItem() {
  const key = newStorageKey.value.trim()
  const value = newStorageValue.value.trim()

  if (!key) {
    alert('请输入 Key')
    return
  }
  if (!value) {
    alert('请输入 Value')
    return
  }

  const exists = websiteForm.value.local_storage.some(item => item.key === key)
  if (exists) {
    alert('该 Key 已存在')
    return
  }

  websiteForm.value.local_storage.push({ key, value })
  newStorageKey.value = ''
  newStorageValue.value = ''
}

function removeStorageItem(index: number) {
  websiteForm.value.local_storage.splice(index, 1)
}

async function saveWebsite() {
  if (!websiteForm.value.name.trim()) {
    alert('请输入网站名称')
    return
  }
  if (!websiteForm.value.base_url.trim()) {
    alert('请输入网站地址')
    return
  }

  isSaving.value = true
  try {
    const website: Website = {
      id: editingWebsite.value?.id || crypto.randomUUID(),
      name: websiteForm.value.name.trim(),
      base_url: websiteForm.value.base_url.trim(),
      spider: websiteForm.value.spider,
      local_storage: websiteForm.value.local_storage,
      is_default: editingWebsite.value?.is_default || false
    }
    await invoke('save_website', { website })
    await loadWebsites()
    closeWebsiteForm()
    showSaveMessage('success', '网站已保存')
  } catch (e) {
    showSaveMessage('error', '保存失败: ' + e)
  } finally {
    isSaving.value = false
  }
}

async function deleteWebsite(id: string) {
  const name = websites.value.find(w => w.id === id)?.name || ''
  if (!confirm(`确定要删除网站 "${name}" 吗？`)) return

  try {
    await invoke('delete_website', { websiteId: id })
    await loadWebsites()
    showSaveMessage('success', '网站已删除')
  } catch (e) {
    showSaveMessage('error', '删除失败: ' + e)
  }
}

async function setDefaultWebsite(id: string) {
  try {
    await invoke('set_default_website', { websiteId: id })
    await loadWebsites()
    showSaveMessage('success', '已设为默认网站')
  } catch (e) {
    showSaveMessage('error', '设置失败: ' + e)
  }
}

// ========== 下载设置 ==========

async function saveDownloadConfig() {
  isSaving.value = true
  try {
    await invoke('update_config', { config: config.value })
    showSaveMessage('success', '配置已保存')
  } catch (e) {
    showSaveMessage('error', '保存失败: ' + e)
  } finally {
    isSaving.value = false
  }
}

async function selectDownloadPath() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择下载目录'
    })
    if (selected) {
      if (Array.isArray(selected) && selected.length > 0) {
        config.value.download_path = selected[0]
      } else if (typeof selected === 'string') {
        config.value.download_path = selected
      }
    }
  } catch (e) {
    console.error('选择目录失败:', e)
  }
}

async function checkFfmpeg() {
  try {
    const hasFfmpeg = await invoke<boolean>('check_ffmpeg')
    if (hasFfmpeg) {
      alert('FFmpeg 已安装')
    } else {
      alert('FFmpeg 未安装\n\n请运行: brew install ffmpeg')
    }
  } catch (e) {
    alert('检查失败: ' + e)
  }
}

// ========== yt-dlp 设置 ==========

async function saveYtdlpConfig() {
  isSaving.value = true
  try {
    await invoke('update_ytdlp_config', { config: ytdlpConfig.value })
    showSaveMessage('success', 'yt-dlp 配置已保存')
  } catch (e) {
    showSaveMessage('error', '保存失败: ' + e)
  } finally {
    isSaving.value = false
  }
}
</script>

<template>
  <div class="settings-page">
    <!-- 保存消息提示 -->
    <Transition name="fade">
      <div v-if="saveMessage" :class="['save-message', saveMessage.type]">
        {{ saveMessage.text }}
      </div>
    </Transition>

    <!-- 左侧导航 -->
    <aside class="settings-nav">
      <div class="nav-header">
        <h2>设置</h2>
      </div>
      <nav class="nav-list">
        <button
          v-for="item in navItems"
          :key="item.id"
          :class="['nav-item', { active: activeSection === item.id }]"
          @click="activeSection = item.id"
        >
          {{ item.label }}
        </button>
      </nav>
    </aside>

    <!-- 右侧内容 -->
    <main class="settings-content">
      <div v-if="isLoading" class="loading">
        <div class="spinner"></div>
        <p>加载中...</p>
      </div>

      <template v-else>
        <!-- 网站设置 -->
        <div v-if="activeSection === 'websites'" class="section">
          <div class="section-header">
            <h3>网站设置</h3>
            <p>管理用于爬取视频的网站配置</p>
          </div>

          <div v-if="!showWebsiteForm" class="content-wrapper">
            <div class="section-toolbar">
              <h4>已配置网站</h4>
              <button @click="openWebsiteForm()" class="add-btn">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="12" y1="5" x2="12" y2="19"/>
                  <line x1="5" y1="12" x2="19" y2="12"/>
                </svg>
                添加网站
              </button>
            </div>

            <div v-if="websites.length === 0" class="empty-state">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <circle cx="12" cy="12" r="10"/>
                <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10"/>
              </svg>
              <p>暂无配置网站</p>
              <p class="hint">点击上方按钮添加第一个网站</p>
            </div>

            <div v-else class="website-list">
              <div v-for="website in websites" :key="website.id" class="website-item">
                <div class="item-main">
                  <div class="item-info">
                    <span class="item-name">{{ website.name }}</span>
                    <span v-if="website.is_default" class="default-badge">默认</span>
                  </div>
                  <span class="item-url">{{ website.base_url }}</span>
                </div>
                <div class="item-actions">
                  <button v-if="!website.is_default" @click="setDefaultWebsite(website.id)" class="action-btn" title="设为默认">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
                    </svg>
                  </button>
                  <button @click="openWebsiteForm(website)" class="action-btn" title="编辑">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                      <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                    </svg>
                  </button>
                  <button @click="deleteWebsite(website.id)" class="action-btn delete" title="删除">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="3 6 5 6 21 6"/>
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                    </svg>
                  </button>
                </div>
              </div>
            </div>
          </div>

          <!-- 网站表单 -->
          <div v-else class="content-wrapper">
            <div class="section-toolbar">
              <button @click="closeWebsiteForm" class="back-btn">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="19" y1="12" x2="5" y2="12"/>
                  <polyline points="12 19 5 12 12 5"/>
                </svg>
                返回
              </button>
              <h4>{{ editingWebsite ? '编辑网站' : '添加网站' }}</h4>
            </div>

            <div class="form-stack">
              <div class="form-group">
                <label>网站名称</label>
                <input type="text" v-model="websiteForm.name" placeholder="例如: 网站A" class="form-input" />
              </div>

              <div class="form-group">
                <label>网站地址</label>
                <input type="text" v-model="websiteForm.base_url" placeholder="例如: https://example.com/" class="form-input" />
              </div>

              <div class="form-group">
                <label>爬虫</label>
                <select v-model="websiteForm.spider" class="form-input">
                  <option v-for="scraper in scrapers" :key="scraper.id" :value="scraper.id">
                    {{ scraper.name }}
                  </option>
                </select>
              </div>

              <div class="form-group">
                <label>LocalStorage 配置</label>
                <div class="add-storage-form">
                  <input type="text" v-model="newStorageKey" placeholder="Key" class="storage-input" />
                  <input type="text" v-model="newStorageValue" placeholder="Value" class="storage-input" />
                  <button @click="addStorageItem" class="add-btn-small">添加</button>
                </div>

                <div v-if="websiteForm.local_storage.length > 0" class="storage-table">
                  <div class="table-row header">
                    <span class="col-key">Key</span>
                    <span class="col-value">Value</span>
                    <span class="col-action"></span>
                  </div>
                  <div class="table-row" v-for="(item, index) in websiteForm.local_storage" :key="index">
                    <span class="col-key">{{ item.key }}</span>
                    <span class="col-value">{{ item.value }}</span>
                    <span class="col-action">
                      <button @click="removeStorageItem(index)" class="delete-btn">&times;</button>
                    </span>
                  </div>
                </div>
              </div>

              <div class="form-actions">
                <button @click="closeWebsiteForm" class="btn-secondary">取消</button>
                <button @click="saveWebsite" :disabled="isSaving" class="btn-primary">
                  {{ isSaving ? '保存中...' : '保存' }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- 下载设置 -->
        <div v-if="activeSection === 'download'" class="section">
          <div class="section-header">
            <h3>下载设置</h3>
            <p>配置 M3U8 视频下载相关的选项</p>
          </div>

          <div class="content-wrapper">
            <div class="form-stack">
              <div class="form-group">
                <label>下载目录</label>
                <div class="input-with-btn">
                  <input type="text" v-model="config.download_path" placeholder="输入下载目录路径" class="form-input" />
                  <button @click="selectDownloadPath" class="btn-secondary">选择</button>
                </div>
              </div>

              <div class="form-group">
                <label>FFmpeg 状态</label>
                <button @click="checkFfmpeg" class="btn-secondary">检查状态</button>
              </div>

              <div class="form-actions">
                <button @click="saveDownloadConfig" :disabled="isSaving" class="btn-primary">
                  {{ isSaving ? '保存中...' : '保存配置' }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- yt-dlp 设置 -->
        <div v-if="activeSection === 'ytdlp'" class="section">
          <div class="section-header">
            <h3>yt-dlp 设置</h3>
            <p>配置 YouTube、B站等平台视频下载的默认选项</p>
          </div>

          <div class="content-wrapper">
            <div class="form-stack">
              <div class="form-row">
                <div class="form-group">
                  <label>视频质量</label>
                  <select v-model="ytdlpConfig.quality" class="form-input">
                    <option value="best">最佳质量</option>
                    <option value="high">1080p 高清</option>
                    <option value="medium">720p 中等</option>
                    <option value="low">480p 流畅</option>
                    <option value="worst">最差质量</option>
                    <option value="audio_only">仅音频</option>
                  </select>
                </div>

                <div class="form-group">
                  <label>视频格式</label>
                  <select v-model="ytdlpConfig.format" class="form-input">
                    <option value="mp4">MP4</option>
                    <option value="webm">WebM</option>
                    <option value="mkv">MKV</option>
                  </select>
                </div>

                <div class="form-group">
                  <label>音频格式</label>
                  <select v-model="ytdlpConfig.audio_format" class="form-input">
                    <option value="mp3">MP3</option>
                    <option value="m4a">M4A</option>
                    <option value="wav">WAV</option>
                    <option value="flac">FLAC</option>
                  </select>
                </div>

                <div class="form-group">
                  <label>并发下载数</label>
                  <input type="number" v-model="ytdlpConfig.concurrent_downloads" min="1" max="5" class="form-input" />
                </div>
              </div>

              <div class="form-divider"></div>

              <div class="form-group-inline">
                <label class="checkbox-label">
                  <input type="checkbox" v-model="ytdlpConfig.audio_only" />
                  <span>仅下载音频</span>
                </label>

                <label class="checkbox-label">
                  <input type="checkbox" v-model="ytdlpConfig.merge_video" />
                  <span>合并视频流</span>
                </label>

                <label class="checkbox-label">
                  <input type="checkbox" v-model="ytdlpConfig.subtitles" />
                  <span>下载字幕</span>
                </label>

                <label class="checkbox-label">
                  <input type="checkbox" v-model="ytdlpConfig.thumbnail" />
                  <span>下载封面</span>
                </label>
              </div>

              <div v-if="ytdlpConfig.subtitles" class="form-group">
                <label>字幕语言</label>
                <input type="text" v-model="ytdlpConfig.subtitle_langs" placeholder="zh-CN,zh-Hans,zh-Hant,en" class="form-input" />
              </div>

              <div class="form-group">
                <label>额外参数</label>
                <input type="text" v-model="ytdlpConfig.extra_options" placeholder="--write-comments --cookies-from-browser chrome" class="form-input" />
                <span class="form-hint">yt-dlp 支持的其他参数，用空格分隔</span>
              </div>

              <div class="form-actions">
                <button @click="saveYtdlpConfig" :disabled="isSaving" class="btn-primary">
                  {{ isSaving ? '保存中...' : '保存配置' }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- 代理设置 -->
        <div v-if="activeSection === 'proxy'" class="section">
          <div class="section-header">
            <h3>代理设置</h3>
            <p>配置网络代理以访问受限网站</p>
          </div>

          <div class="content-wrapper">
            <div class="coming-soon">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
                <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
              </svg>
              <p>代理设置功能开发中</p>
            </div>
          </div>
        </div>

        <!-- AI 设置 -->
        <div v-if="activeSection === 'ai'" class="section">
          <div class="section-header">
            <h3>AI 设置</h3>
            <p>配置 AI 相关功能</p>
          </div>

          <div class="content-wrapper">
            <div class="coming-soon">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path d="M12 2a10 10 0 1 0 10 10"/>
                <path d="M12 16v-4"/>
                <path d="M12 8h.01"/>
              </svg>
              <p>AI 设置功能开发中</p>
            </div>
          </div>
        </div>
      </template>
    </main>
  </div>
</template>

<style scoped>
.settings-page {
  height: 100%;
  display: flex;
  background: #f5f6f8;
  border-radius: 12px;
  overflow: hidden;
}

/* 保存消息提示 */
.save-message {
  position: fixed;
  top: 20px;
  right: 20px;
  padding: 12px 20px;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  z-index: 1000;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.save-message.success {
  background: #f0fdf4;
  color: #166534;
  border: 1px solid #bbf7d0;
}

.save-message.error {
  background: #fef2f2;
  color: #991b1b;
  border: 1px solid #fecaca;
}

.fade-enter-active, .fade-leave-active {
  transition: all 0.3s ease;
}

.fade-enter-from, .fade-leave-to {
  opacity: 0;
  transform: translateX(20px);
}

/* 左侧导航 */
.settings-nav {
  width: 160px;
  background: #fff;
  border-right: 1px solid #e5e7eb;
  flex-shrink: 0;
}

.nav-header {
  padding: 16px 12px;
  border-bottom: 1px solid #e5e7eb;
}

.nav-header h2 {
  font-size: 16px;
  font-weight: 600;
  color: #1a1a2e;
  margin: 0;
}

.nav-list {
  padding: 12px 8px;
}

.nav-item {
  display: block;
  width: 100%;
  padding: 10px 12px;
  background: transparent;
  border: none;
  border-radius: 6px;
  font-size: 14px;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s;
  text-align: left;
}

.nav-item:hover {
  background: #f5f6f8;
  color: #1a1a2e;
}

.nav-item.active {
  background: #eef2ff;
  color: #667eea;
  font-weight: 500;
}

/* 右侧内容 */
.settings-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px 30px;
}

.loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #94a3b8;
}

.spinner {
  width: 32px;
  height: 32px;
  border: 3px solid #e5e7eb;
  border-top-color: #667eea;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.section {
  width: 100%;
  max-width: none;
}

.section-header {
  margin-bottom: 16px;
}

.section-header h3 {
  font-size: 18px;
  font-weight: 600;
  color: #1a1a2e;
  margin: 0 0 4px;
}

.section-header p {
  font-size: 13px;
  color: #94a3b8;
  margin: 0;
}

.content-wrapper {
  background: #fff;
  border-radius: 10px;
  border: 1px solid #e5e7eb;
  padding: 16px;
  width: 100%;
  box-sizing: border-box;
}

.section-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid #f0f0f0;
}

.section-toolbar h4 {
  font-size: 15px;
  font-weight: 600;
  color: #1a1a2e;
  margin: 0;
}

.back-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  background: #f5f6f8;
  color: #64748b;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
}

.back-btn:hover {
  background: #e5e7eb;
}

.add-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
}

.add-btn:hover {
  background: #5a67d8;
}

.empty-state {
  text-align: center;
  padding: 40px 20px;
  color: #94a3b8;
}

.empty-state svg {
  opacity: 0.4;
  margin-bottom: 12px;
}

.empty-state .hint {
  font-size: 12px;
  margin-top: 8px;
}

/* 网站列表 */
.website-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.website-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px;
  background: #fafbfc;
  border-radius: 8px;
  border: 1px solid #f0f0f0;
}

.website-item:hover {
  border-color: #e5e7eb;
}

.item-main {
  flex: 1;
  min-width: 0;
}

.item-info {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.item-name {
  font-size: 14px;
  font-weight: 500;
  color: #1a1a2e;
}

.default-badge {
  padding: 2px 8px;
  background: #eef2ff;
  color: #667eea;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
}

.item-url {
  font-size: 12px;
  color: #94a3b8;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.item-actions {
  display: flex;
  gap: 4px;
  margin-left: 12px;
}

.action-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: #94a3b8;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.action-btn:hover {
  background: #fff;
  color: #667eea;
}

.action-btn.delete:hover {
  background: #fee2e2;
  color: #dc2626;
}

/* 表单样式 */
.form-stack {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.form-row {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
}

@media (max-width: 600px) {
  .form-row {
    grid-template-columns: 1fr;
  }
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-group label {
  font-size: 13px;
  font-weight: 500;
  color: #374151;
}

.form-input {
  padding: 10px 12px;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  font-size: 14px;
  color: #1a1a2e;
  transition: all 0.2s;
}

.form-input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

/* Select dropdown styling */
select.form-input {
  appearance: none;
  -webkit-appearance: none;
  -moz-appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%2364748b' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 12px center;
  padding-right: 36px;
  cursor: pointer;
}

select.form-input:hover {
  border-color: #d1d5db;
}

select.form-input:focus {
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.form-hint {
  font-size: 12px;
  color: #94a3b8;
}

.input-with-btn {
  display: flex;
  gap: 8px;
}

.input-with-btn .form-input {
  flex: 1;
}

.form-divider {
  height: 1px;
  background: #f0f0f0;
  margin: 4px 0;
}

.form-group-inline {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  font-size: 14px;
  color: #374151;
}

.checkbox-label input {
  width: 16px;
  height: 16px;
  accent-color: #667eea;
}

.btn-primary {
  padding: 10px 20px;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 6px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-primary:hover:not(:disabled) {
  background: #5a67d8;
}

.btn-primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-secondary {
  padding: 10px 16px;
  background: #f5f6f8;
  color: #64748b;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  font-size: 14px;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-secondary:hover {
  background: #e5e7eb;
  color: #1a1a2e;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding-top: 12px;
  border-top: 1px solid #f0f0f0;
}

/* LocalStorage 表单 */
.add-storage-form {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}

.storage-input {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  font-size: 13px;
}

.storage-input:focus {
  outline: none;
  border-color: #667eea;
}

.add-btn-small {
  padding: 8px 14px;
  background: #f5f6f8;
  color: #64748b;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
}

.add-btn-small:hover {
  background: #667eea;
  color: white;
  border-color: #667eea;
}

.storage-table {
  border: 1px solid #f0f0f0;
  border-radius: 6px;
  overflow: hidden;
}

.table-row {
  display: flex;
  align-items: center;
  padding: 10px 12px;
  border-bottom: 1px solid #f5f5f5;
}

.table-row:last-child {
  border-bottom: none;
}

.table-row.header {
  background: #fafbfc;
  font-size: 12px;
  font-weight: 600;
  color: #64748b;
}

.col-key {
  width: 100px;
  font-size: 13px;
  color: #1a1a2e;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-value {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: #64748b;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-action {
  width: 32px;
  display: flex;
  justify-content: center;
}

.delete-btn {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: #94a3b8;
  border: none;
  border-radius: 4px;
  font-size: 16px;
  cursor: pointer;
}

.delete-btn:hover {
  background: #fee2e2;
  color: #dc2626;
}

/* 即将推出 */
.coming-soon {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 60px 20px;
  text-align: center;
  color: #94a3b8;
}

.coming-soon svg {
  opacity: 0.4;
  margin-bottom: 12px;
}

.coming-soon p {
  font-size: 14px;
  margin: 0;
}
</style>
