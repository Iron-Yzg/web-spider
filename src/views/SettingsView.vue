<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { open, message, ask } from '@tauri-apps/plugin-dialog'
import type { AppConfig, Website, LocalStorageItem, ScraperInfo, YtdlpConfig } from '../types'
import {
  getConfig,
  updateConfig,
  getWebsites,
  saveWebsite as saveWebsiteApi,
  deleteWebsite as deleteWebsiteApi,
  setDefaultWebsite as setDefaultWebsiteApi,
  getScrapers,
  getYtdlpConfig,
  updateYtdlpConfig,
} from '../services/api'

// 设置分类
type SettingSection = 'websites' | 'ytdlp' | 'proxy' | 'ai'

interface NavItem {
  id: SettingSection
  label: string
}

// 导航列表（无图标）
const navItems: NavItem[] = [
  { id: 'websites', label: '网站设置' },
  { id: 'ytdlp', label: '下载设置' },
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
  quality: 720,  // 0=最佳，480/720/1080=对应分辨率，2160=4K
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
    config.value = await getConfig()
  } catch (e) {
    console.error('加载配置失败:', e)
  }
}

async function loadYtdlpConfig() {
  try {
    const saved = await getYtdlpConfig()
    if (saved) {
      ytdlpConfig.value = saved
    }
  } catch (e) {
    console.error('加载 yt-dlp 配置失败:', e)
  }
}

async function loadWebsites() {
  try {
    websites.value = await getWebsites()
  } catch (e) {
    console.error('加载网站列表失败:', e)
  }
}

async function loadScrapers() {
  try {
    scrapers.value = await getScrapers()
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
    await saveWebsiteApi(website)
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
  const confirmed = await ask(`确定要删除网站 "${name}" 吗？`, {
    title: '确认删除',
    kind: 'warning',
  })
  if (!confirmed) return

  try {
    await deleteWebsiteApi(id)
    await loadWebsites()
    await message('网站已删除', { title: '成功', kind: 'info' })
  } catch (e) {
    await message(`删除失败: ${e}`, { title: '错误', kind: 'error' })
  }
}

async function setDefaultWebsite(id: string) {
  try {
    await setDefaultWebsiteApi(id)
    await loadWebsites()
    showSaveMessage('success', '已设为默认网站')
  } catch (e) {
    showSaveMessage('error', '设置失败: ' + e)
  }
}

async function selectDownloadPath() {
  try {
    const selected = await open({
      title: '选择下载目录',
      directory: true,
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

// ========== yt-dlp 设置 ==========

async function saveYtdlpConfig() {
  isSaving.value = true
  try {
    await updateConfig({ download_path: config.value.download_path })
    await updateYtdlpConfig(ytdlpConfig.value)
    showSaveMessage('success', '下载与 yt-dlp 配置已保存')
  } catch (e) {
    showSaveMessage('error', '保存失败: ' + e)
  } finally {
    isSaving.value = false
  }
}
</script>

<template>
  <div class="flex h-full overflow-hidden rounded-xl bg-[#f5f6f8]">
    <!-- 保存消息提示 -->
    <Transition name="fade">
      <div
        v-if="saveMessage"
        :class="[
          'fixed left-1/2 top-[70px] z-[1000] -translate-x-1/2 rounded-lg px-6 py-2.5 text-sm font-medium shadow-[0_4px_12px_rgba(0,0,0,0.15)]',
          saveMessage.type === 'success'
            ? 'border border-[#bbf7d0] bg-[#f0fdf4] text-[#166534]'
            : 'border border-[#fecaca] bg-[#fef2f2] text-[#991b1b]'
        ]"
      >
        {{ saveMessage.text }}
      </div>
    </Transition>

    <!-- 左侧导航 -->
    <aside class="w-40 shrink-0 border-r border-[#e5e7eb] bg-white">
      <div class="border-b border-[#e5e7eb] px-3 py-4">
        <h2 class="m-0 text-base font-semibold text-[#1a1a2e]">设置</h2>
      </div>
      <nav class="p-2">
        <button
          v-for="item in navItems"
          :key="item.id"
          :class="[
            'block w-full rounded-md border-none bg-transparent px-3 py-2.5 text-left text-sm transition-all',
            activeSection === item.id
              ? 'bg-[#eef2ff] font-medium text-[#667eea]'
              : 'text-[#64748b] hover:bg-[#f5f6f8] hover:text-[#1a1a2e]'
          ]"
          @click="activeSection = item.id"
        >
          {{ item.label }}
        </button>
      </nav>
    </aside>

    <!-- 右侧内容 -->
    <main class="flex-1 overflow-y-auto px-[30px] py-5">
      <div v-if="isLoading" class="flex h-full flex-col items-center justify-center text-[#94a3b8]">
        <div class="h-8 w-8 animate-spin rounded-full border-[3px] border-[#e5e7eb] border-t-[#667eea]"></div>
        <p>加载中...</p>
      </div>

      <template v-else>
        <!-- 网站设置 -->
        <div v-if="activeSection === 'websites'" class="w-full max-w-none">
          <div class="mb-4">
            <p class="m-0 text-[13px] text-[#94a3b8]">管理用于爬取视频的网站配置</p>
          </div>

          <div v-if="!showWebsiteForm" class="box-border w-full rounded-[10px] border border-[#e5e7eb] bg-white p-4">
            <div class="mb-4 flex items-center justify-between border-b border-[#f0f0f0] pb-3">
              <h4 class="m-0 text-[15px] font-semibold text-[#1a1a2e]">已配置网站</h4>
              <button
                @click="openWebsiteForm()"
                class="inline-flex items-center gap-1 rounded-md border-none bg-[#667eea] px-3 py-1.5 text-[13px] font-medium text-white transition-colors hover:bg-[#5a67d8]"
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="12" y1="5" x2="12" y2="19"/>
                  <line x1="5" y1="12" x2="19" y2="12"/>
                </svg>
                添加网站
              </button>
            </div>

            <div v-if="websites.length === 0" class="px-5 py-10 text-center text-[#94a3b8]">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <circle cx="12" cy="12" r="10"/>
                <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10"/>
              </svg>
              <p>暂无配置网站</p>
              <p class="mt-2 text-xs">点击上方按钮添加第一个网站</p>
            </div>

            <div v-else class="flex flex-col gap-2">
              <div
                v-for="website in websites"
                :key="website.id"
                class="flex items-center justify-between rounded-lg border border-[#f0f0f0] bg-[#fafbfc] p-3 hover:border-[#e5e7eb]"
              >
                <div class="min-w-0 flex-1">
                  <div class="mb-1 flex items-center gap-2">
                    <span class="text-sm font-medium text-[#1a1a2e]">{{ website.name }}</span>
                    <span v-if="website.is_default" class="rounded bg-[#eef2ff] px-2 py-0.5 text-[11px] font-medium text-[#667eea]">默认</span>
                  </div>
                  <span class="block overflow-hidden text-ellipsis whitespace-nowrap text-xs text-[#94a3b8]">{{ website.base_url }}</span>
                </div>
                <div class="ml-3 flex gap-1">
                  <button
                    v-if="!website.is_default"
                    @click.stop="setDefaultWebsite(website.id)"
                    class="flex h-7 w-7 items-center justify-center rounded-md border-none bg-transparent text-[#94a3b8] transition-all hover:bg-white hover:text-[#667eea]"
                    title="设为默认"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
                    </svg>
                  </button>
                  <button
                    @click.stop="openWebsiteForm(website)"
                    class="flex h-7 w-7 items-center justify-center rounded-md border-none bg-transparent text-[#94a3b8] transition-all hover:bg-white hover:text-[#667eea]"
                    title="编辑"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                      <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                    </svg>
                  </button>
                  <button
                    @click.stop="deleteWebsite(website.id)"
                    class="flex h-7 w-7 items-center justify-center rounded-md border-none bg-transparent text-[#94a3b8] transition-all hover:bg-[#fee2e2] hover:text-[#dc2626]"
                    title="删除"
                  >
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
          <div v-else class="box-border w-full rounded-[10px] border border-[#e5e7eb] bg-white p-4">
            <div class="mb-4 flex items-center justify-between border-b border-[#f0f0f0] pb-3">
              <button
                @click="closeWebsiteForm"
                class="inline-flex items-center gap-1 rounded-md border-none bg-[#f5f6f8] px-3 py-1.5 text-[13px] text-[#64748b] transition-colors hover:bg-[#e5e7eb]"
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="19" y1="12" x2="5" y2="12"/>
                  <polyline points="12 19 5 12 12 5"/>
                </svg>
                返回
              </button>
              <h4 class="m-0 text-[15px] font-semibold text-[#1a1a2e]">{{ editingWebsite ? '编辑网站' : '添加网站' }}</h4>
            </div>

            <div class="flex flex-col gap-4">
              <div class="flex flex-col gap-1.5">
                <label class="text-[13px] font-medium text-[#374151]">网站名称</label>
                <input
                  type="text"
                  v-model="websiteForm.name"
                  placeholder="例如: 网站A"
                  class="rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                />
              </div>

              <div class="flex flex-col gap-1.5">
                <label class="text-[13px] font-medium text-[#374151]">网站地址</label>
                <input
                  type="text"
                  v-model="websiteForm.base_url"
                  placeholder="例如: https://example.com/"
                  class="rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                />
              </div>

              <div class="flex flex-col gap-1.5">
                <label class="text-[13px] font-medium text-[#374151]">爬虫</label>
                <select
                  v-model="websiteForm.spider"
                  class="select-modern cursor-pointer rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all hover:border-[#d1d5db] focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                >
                  <option v-for="scraper in scrapers" :key="scraper.id" :value="scraper.id">
                    {{ scraper.name }}
                  </option>
                </select>
              </div>

              <div class="flex flex-col gap-1.5">
                <label class="text-[13px] font-medium text-[#374151]">LocalStorage 配置</label>
                <div class="mb-3 flex gap-2">
                  <input
                    type="text"
                    v-model="newStorageKey"
                    placeholder="Key"
                    class="flex-1 rounded-md border border-[#e5e7eb] px-3 py-2 text-[13px] focus:border-[#667eea] focus:outline-none"
                  />
                  <input
                    type="text"
                    v-model="newStorageValue"
                    placeholder="Value"
                    class="flex-1 rounded-md border border-[#e5e7eb] px-3 py-2 text-[13px] focus:border-[#667eea] focus:outline-none"
                  />
                  <button
                    @click="addStorageItem"
                    class="rounded-md border border-[#e5e7eb] bg-[#f5f6f8] px-3.5 py-2 text-[13px] text-[#64748b] transition-all hover:border-[#667eea] hover:bg-[#667eea] hover:text-white"
                  >添加</button>
                </div>

                <div v-if="websiteForm.local_storage.length > 0" class="overflow-hidden rounded-md border border-[#f0f0f0]">
                  <div class="flex items-center border-b border-[#f5f5f5] bg-[#fafbfc] px-3 py-2.5 text-xs font-semibold text-[#64748b]">
                    <span class="w-[100px] overflow-hidden text-ellipsis whitespace-nowrap text-[13px] text-[#1a1a2e]">Key</span>
                    <span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-xs text-[#64748b]">Value</span>
                    <span class="flex w-8 justify-center"></span>
                  </div>
                  <div
                    v-for="(item, index) in websiteForm.local_storage"
                    :key="index"
                    class="flex items-center border-b border-[#f5f5f5] px-3 py-2.5 last:border-b-0"
                  >
                    <span class="w-[100px] overflow-hidden text-ellipsis whitespace-nowrap text-[13px] text-[#1a1a2e]">{{ item.key }}</span>
                    <span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-xs text-[#64748b]">{{ item.value }}</span>
                    <span class="flex w-8 justify-center">
                      <button
                        @click="removeStorageItem(index)"
                        class="flex h-6 w-6 items-center justify-center rounded text-base text-[#94a3b8] transition-all hover:bg-[#fee2e2] hover:text-[#dc2626]"
                      >&times;</button>
                    </span>
                  </div>
                </div>
              </div>

              <div class="flex justify-end gap-3 border-t border-[#f0f0f0] pt-3">
                <button
                  @click="closeWebsiteForm"
                  class="rounded-md border border-[#e5e7eb] bg-[#f5f6f8] px-4 py-2.5 text-sm text-[#64748b] transition-all hover:bg-[#e5e7eb] hover:text-[#1a1a2e]"
                >取消</button>
                <button
                  @click="saveWebsite"
                  :disabled="isSaving"
                  class="rounded-md border-none bg-[#667eea] px-5 py-2.5 text-sm font-medium text-white transition-all hover:bg-[#5a67d8] disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {{ isSaving ? '保存中...' : '保存' }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- yt-dlp 设置 -->
        <div v-if="activeSection === 'ytdlp'" class="w-full max-w-none">
          <div class="mb-4">
            <p class="m-0 text-[13px] text-[#94a3b8]">配置 YouTube、B站等平台视频下载的默认选项</p>
          </div>

          <div class="box-border w-full rounded-[10px] border border-[#e5e7eb] bg-white p-4">
            <div class="flex flex-col gap-4">
              <div class="flex flex-col gap-1.5">
                <label class="text-[13px] font-medium text-[#374151]">下载目录</label>
                <div class="flex gap-2">
                  <input
                    type="text"
                    v-model="config.download_path"
                    placeholder="输入下载目录路径"
                    class="flex-1 rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                  />
                  <button
                    @click="selectDownloadPath"
                    class="rounded-md border border-[#e5e7eb] bg-[#f5f6f8] px-4 py-2.5 text-sm text-[#64748b] transition-all hover:bg-[#e5e7eb] hover:text-[#1a1a2e]"
                  >选择</button>
                </div>
              </div>

              <div class="my-1 h-px bg-[#f0f0f0]"></div>

              <div class="grid grid-cols-2 gap-4 max-[600px]:grid-cols-1">
                <div class="flex flex-col gap-1.5">
                  <label class="text-[13px] font-medium text-[#374151]">视频质量</label>
                  <select
                    v-model="ytdlpConfig.quality"
                    class="select-modern cursor-pointer rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all hover:border-[#d1d5db] focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                  >
                    <option :value="720">最佳质量</option>
                    <option :value="2160">4K (2160p)</option>
                    <option :value="1080">1080p 高清</option>
                    <option :value="720">720p 中等</option>
                    <option :value="480">480p 流畅</option>
                    <option :value="360">360p 流畅</option>
                  </select>
                </div>

                <div class="flex flex-col gap-1.5">
                  <label class="text-[13px] font-medium text-[#374151]">视频格式</label>
                  <select
                    v-model="ytdlpConfig.format"
                    class="select-modern cursor-pointer rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all hover:border-[#d1d5db] focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                  >
                    <option value="mp4">MP4</option>
                    <option value="webm">WebM</option>
                    <option value="mkv">MKV</option>
                  </select>
                </div>

                <div class="flex flex-col gap-1.5">
                  <label class="text-[13px] font-medium text-[#374151]">音频格式</label>
                  <select
                    v-model="ytdlpConfig.audio_format"
                    class="select-modern cursor-pointer rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all hover:border-[#d1d5db] focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                  >
                    <option value="mp3">MP3</option>
                    <option value="m4a">M4A</option>
                    <option value="wav">WAV</option>
                    <option value="flac">FLAC</option>
                  </select>
                </div>

                <div class="flex flex-col gap-1.5">
                  <label class="text-[13px] font-medium text-[#374151]">并发下载数</label>
                  <input
                    type="number"
                    v-model="ytdlpConfig.concurrent_downloads"
                    min="1"
                    max="5"
                    class="rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                  />
                </div>
              </div>

              <div class="my-1 h-px bg-[#f0f0f0]"></div>

              <div class="flex flex-wrap gap-4">
                <label class="flex cursor-pointer items-center gap-2 text-sm text-[#374151]">
                  <input type="checkbox" v-model="ytdlpConfig.audio_only" class="h-4 w-4 accent-[#667eea]" />
                  <span>仅下载音频</span>
                </label>

                <label class="flex cursor-pointer items-center gap-2 text-sm text-[#374151]">
                  <input type="checkbox" v-model="ytdlpConfig.merge_video" class="h-4 w-4 accent-[#667eea]" />
                  <span>合并视频流</span>
                </label>

                <label class="flex cursor-pointer items-center gap-2 text-sm text-[#374151]">
                  <input type="checkbox" v-model="ytdlpConfig.subtitles" class="h-4 w-4 accent-[#667eea]" />
                  <span>下载字幕</span>
                </label>

                <label class="flex cursor-pointer items-center gap-2 text-sm text-[#374151]">
                  <input type="checkbox" v-model="ytdlpConfig.thumbnail" class="h-4 w-4 accent-[#667eea]" />
                  <span>下载封面</span>
                </label>
              </div>

              <div v-if="ytdlpConfig.subtitles" class="flex flex-col gap-1.5">
                <label class="text-[13px] font-medium text-[#374151]">字幕语言</label>
                <input
                  type="text"
                  v-model="ytdlpConfig.subtitle_langs"
                  placeholder="zh-CN,zh-Hans,zh-Hant,en"
                  class="rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                />
              </div>

              <div class="flex flex-col gap-1.5">
                <label class="text-[13px] font-medium text-[#374151]">额外参数</label>
                <input
                  type="text"
                  v-model="ytdlpConfig.extra_options"
                  placeholder="--write-comments --cookies-from-browser chrome"
                  class="rounded-md border border-[#e5e7eb] px-3 py-2.5 text-sm text-[#1a1a2e] transition-all focus:border-[#667eea] focus:outline-none focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]"
                />
                <span class="text-xs text-[#94a3b8]">yt-dlp 支持的其他参数，用空格分隔</span>
              </div>

              <div class="flex justify-end gap-3 border-t border-[#f0f0f0] pt-3">
                <button
                  @click="saveYtdlpConfig"
                  :disabled="isSaving"
                  class="rounded-md border-none bg-[#667eea] px-5 py-2.5 text-sm font-medium text-white transition-all hover:bg-[#5a67d8] disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {{ isSaving ? '保存中...' : '保存配置' }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- 代理设置 -->
        <div v-if="activeSection === 'proxy'" class="w-full max-w-none">
          <div class="mb-4">
            <p class="m-0 text-[13px] text-[#94a3b8]">配置网络代理以访问受限网站</p>
          </div>

          <div class="box-border w-full rounded-[10px] border border-[#e5e7eb] bg-white p-4">
            <div class="flex flex-col items-center justify-center px-5 py-[60px] text-center text-[#94a3b8]">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
                <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
              </svg>
              <p class="m-0 text-sm">代理设置功能开发中</p>
            </div>
          </div>
        </div>

        <!-- AI 设置 -->
        <div v-if="activeSection === 'ai'" class="w-full max-w-none">
          <div class="mb-4">
            <p class="m-0 text-[13px] text-[#94a3b8]">配置 AI 相关功能</p>
          </div>

          <div class="box-border w-full rounded-[10px] border border-[#e5e7eb] bg-white p-4">
            <div class="flex flex-col items-center justify-center px-5 py-[60px] text-center text-[#94a3b8]">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path d="M12 2a10 10 0 1 0 10 10"/>
                <path d="M12 16v-4"/>
                <path d="M12 8h.01"/>
              </svg>
              <p class="m-0 text-sm">AI 设置功能开发中</p>
            </div>
          </div>
        </div>
      </template>
    </main>
  </div>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: all 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: translateX(20px);
}
</style>
