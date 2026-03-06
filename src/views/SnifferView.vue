<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import type { SniffedMedia, SniffResult, SniffedMediaRecord } from '../types'
import { sniffMedia, addYtdlpTasks, getSniffedRecords, clearSniffedRecords } from '../services/api'
import IconButton from '../components/IconButton.vue'
import VideoPlayer from '../components/VideoPlayer.vue'

// 输入 URL
const url = ref('')
const isSniffing = ref(false)
const sniffResult = ref<SniffResult | null>(null)
const logs = ref<string[]>([])

// 数据库历史记录
const dbRecords = ref<SniffedMediaRecord[]>([])
const showHistory = ref(false)

// 筛选
const typeFilter = ref<string>('')
const copyMessage = ref('')

// 选中状态
const selectedUrls = ref<Set<string>>(new Set())

// 播放器状态
const playerVisible = ref(false)
const playerSrc = ref('')
const playerTitle = ref('')

let unlistenLog: (() => void) | null = null

onMounted(async () => {
  // 加载数据库中的历史记录
  await loadDbRecords()

  // 监听嗅探日志
  unlistenLog = await listen<string>('sniff-log', (event) => {
    logs.value.push(event.payload)
  })
})

onUnmounted(() => {
  if (unlistenLog) unlistenLog()
})

// 加载数据库历史
async function loadDbRecords() {
  try {
    dbRecords.value = await getSniffedRecords()
  } catch (e) {
    console.error('加载嗅探历史失败:', e)
  }
}

// 按页面URL分组的历史
const groupedHistory = computed(() => {
  const groups: Record<string, { page_url: string; page_title: string; count: number; time: string; records: SniffedMediaRecord[] }> = {}
  for (const r of dbRecords.value) {
    if (!groups[r.page_url]) {
      groups[r.page_url] = {
        page_url: r.page_url,
        page_title: r.page_title || r.page_url,
        count: 0,
        time: r.sniffed_at,
        records: [],
      }
    }
    groups[r.page_url].count++
    groups[r.page_url].records.push(r)
  }
  return Object.values(groups).sort((a, b) => b.time.localeCompare(a.time))
})

// 筛选后的媒体列表
const filteredMedia = computed(() => {
  if (!sniffResult.value) return []
  let list = sniffResult.value.media_list
  if (typeFilter.value) {
    list = list.filter(m => m.media_type === typeFilter.value || m.file_ext === typeFilter.value)
  }
  return list
})

// 媒体类型统计
const mediaStats = computed(() => {
  if (!sniffResult.value) return {}
  const stats: Record<string, number> = {}
  sniffResult.value.media_list.forEach(m => {
    const key = m.media_type
    stats[key] = (stats[key] || 0) + 1
  })
  return stats
})

// 开始嗅探
async function startSniff() {
  if (!url.value.trim()) return

  let targetUrl = url.value.trim()
  if (!targetUrl.startsWith('http://') && !targetUrl.startsWith('https://')) {
    targetUrl = 'https://' + targetUrl
  }

  isSniffing.value = true
  logs.value = []
  sniffResult.value = null
  selectedUrls.value.clear()
  showHistory.value = false

  try {
    const result = await sniffMedia(targetUrl)
    sniffResult.value = result

    // 刷新数据库历史
    await loadDbRecords()
  } catch (e) {
    sniffResult.value = {
      page_url: targetUrl,
      page_title: '',
      media_list: [],
      success: false,
      message: String(e),
    }
  } finally {
    isSniffing.value = false
  }
}

// 从历史记录恢复
function loadFromHistory(item: { page_url: string }) {
  url.value = item.page_url
  startSniff()
}

// 清除历史
async function clearHistory() {
  try {
    await clearSniffedRecords()
    dbRecords.value = []
  } catch (e) {
    console.error('清除历史失败:', e)
  }
}

// 复制 URL
async function copyUrl(mediaUrl: string) {
  try {
    await navigator.clipboard.writeText(mediaUrl)
    copyMessage.value = '已复制'
    setTimeout(() => { copyMessage.value = '' }, 1500)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = mediaUrl
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
    copyMessage.value = '已复制'
    setTimeout(() => { copyMessage.value = '' }, 1500)
  }
}

// 播放视频
function playMedia(mediaUrl: string, title?: string) {
  playerSrc.value = mediaUrl
  playerTitle.value = title || '嗅探视频'
  playerVisible.value = true
}

// 关闭播放器
function handlePlayerClose() {
  playerVisible.value = false
  playerSrc.value = ''
  playerTitle.value = ''
}

// 添加到下载队列
async function addToDownload(media: SniffedMedia) {
  try {
    await addYtdlpTasks([media.url])
    copyMessage.value = '已添加到下载'
    setTimeout(() => { copyMessage.value = '' }, 1500)
  } catch (e) {
    alert('添加下载失败: ' + e)
  }
}

// 批量添加到下载
async function batchAddToDownload() {
  const urls = Array.from(selectedUrls.value)
  if (urls.length === 0) return

  try {
    await addYtdlpTasks(urls)
    copyMessage.value = `已添加 ${urls.length} 个到下载`
    selectedUrls.value.clear()
    setTimeout(() => { copyMessage.value = '' }, 1500)
  } catch (e) {
    alert('批量添加失败: ' + e)
  }
}

// 批量复制
async function batchCopy() {
  const urls = Array.from(selectedUrls.value)
  if (urls.length === 0) return

  try {
    await navigator.clipboard.writeText(urls.join('\n'))
    copyMessage.value = `已复制 ${urls.length} 个链接`
    setTimeout(() => { copyMessage.value = '' }, 1500)
  } catch { /* ignore */ }
}

// 切换选中
function toggleSelect(mediaUrl: string) {
  if (selectedUrls.value.has(mediaUrl)) {
    selectedUrls.value.delete(mediaUrl)
  } else {
    selectedUrls.value.add(mediaUrl)
  }
}

// 全选/取消全选
function toggleSelectAll() {
  if (selectedUrls.value.size === filteredMedia.value.length) {
    selectedUrls.value.clear()
  } else {
    filteredMedia.value.forEach(m => selectedUrls.value.add(m.url))
  }
}

// 格式化文件大小
function formatSize(bytes: number | null): string {
  if (!bytes || bytes === 0) return '-'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
}

// 获取类型颜色
function getTypeColor(mediaType: string): string {
  const colors: Record<string, string> = {
    hls: 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400',
    dash: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400',
    video: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
    audio: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400',
    stream: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400',
  }
  return colors[mediaType] || 'bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-400'
}

// 获取来源标签
function getSourceLabel(source: string): string {
  const labels: Record<string, string> = {
    dom: '页面元素',
    network: '网络请求',
    script: '脚本内嵌',
    iframe: 'iframe',
    player: '播放器',
  }
  return labels[source] || source
}

// 截断 URL
function truncateUrl(fullUrl: string, maxLen = 80): string {
  if (fullUrl.length <= maxLen) return fullUrl
  return fullUrl.slice(0, maxLen - 3) + '...'
}

// 格式化时间
function formatTime(isoStr: string): string {
  try {
    const d = new Date(isoStr)
    return `${d.getMonth() + 1}/${d.getDate()} ${d.getHours()}:${String(d.getMinutes()).padStart(2, '0')}`
  } catch {
    return isoStr
  }
}
</script>

<template>
  <div class="h-full flex flex-col bg-white dark:bg-gray-900 rounded-xl shadow-[0_2px_12px_rgba(0,0,0,0.06)] overflow-hidden">
    <!-- 顶部输入栏 -->
    <div class="flex gap-2.5 px-5 py-4 border-b border-[#f0f0f0] dark:border-gray-700 shrink-0">
      <div class="flex-1 flex items-center gap-2 px-3.5 py-2 bg-[#f5f6f8] dark:bg-gray-800 rounded-lg">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-[#94a3b8] shrink-0">
          <circle cx="12" cy="12" r="10" />
          <line x1="2" y1="12" x2="22" y2="12" />
          <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
        </svg>
        <input
          type="text"
          v-model="url"
          placeholder="输入网页 URL，自动嗅探页面中的视频/音频资源"
          @keyup.enter="startSniff"
          :disabled="isSniffing"
          class="flex-1 border-none bg-transparent text-sm outline-none text-[#1a1a2e] dark:text-gray-200 placeholder:text-[#94a3b8]"
        />
      </div>
      <button
        @click="startSniff"
        :disabled="isSniffing || !url.trim()"
        class="px-6 py-2.5 text-white border-none rounded-lg text-sm font-medium cursor-pointer whitespace-nowrap transition-all bg-[linear-gradient(135deg,#f59e0b_0%,#d97706_100%)] hover:-translate-y-0.5 hover:shadow-[0_4px_12px_rgba(245,158,11,0.35)] disabled:opacity-60 disabled:cursor-not-allowed"
      >
        <span v-if="isSniffing" class="flex items-center gap-1.5">
          <svg class="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
          嗅探中...
        </span>
        <span v-else class="flex items-center gap-1.5">
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          嗅探
        </span>
      </button>
      <!-- 切换历史记录 -->
      <button
        v-if="groupedHistory.length > 0"
        @click="showHistory = !showHistory"
        :class="['px-3 py-2 border rounded-lg text-sm cursor-pointer transition-all', showHistory ? 'bg-[#667eea] text-white border-[#667eea]' : 'bg-transparent text-[#64748b] dark:text-gray-400 border-[#e8e8e8] dark:border-gray-600 hover:border-[#667eea] hover:text-[#667eea]']"
        title="嗅探历史"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
      </button>
    </div>

    <div class="flex-1 flex overflow-hidden">
      <!-- 主内容区 -->
      <div class="flex-1 flex flex-col overflow-hidden">
        <!-- 结果工具栏 -->
        <div v-if="sniffResult && !showHistory" class="flex justify-between items-center px-5 py-3 bg-[#fafbfc] dark:bg-gray-800 border-b border-[#f0f0f0] dark:border-gray-700 shrink-0">
          <div class="flex items-center gap-3">
            <span class="text-sm font-semibold text-[#1a1a2e] dark:text-gray-200">
              {{ sniffResult.page_title || '嗅探结果' }}
            </span>
            <span class="text-xs text-[#94a3b8]">
              发现 {{ sniffResult.media_list.length }} 个资源
            </span>
            <div class="flex gap-1">
              <button
                :class="['px-2 py-0.5 rounded text-[11px] font-medium cursor-pointer border-none transition-all', !typeFilter ? 'bg-[#667eea] text-white' : 'bg-[#f1f5f9] text-[#64748b] dark:bg-gray-700 dark:text-gray-400 hover:bg-[#e2e8f0]']"
                @click="typeFilter = ''"
              >全部</button>
              <button
                v-for="(count, type_) in mediaStats"
                :key="type_"
                :class="['px-2 py-0.5 rounded text-[11px] font-medium cursor-pointer border-none transition-all', typeFilter === type_ ? 'bg-[#667eea] text-white' : 'bg-[#f1f5f9] text-[#64748b] dark:bg-gray-700 dark:text-gray-400 hover:bg-[#e2e8f0]']"
                @click="typeFilter = String(type_)"
              >{{ type_ }} ({{ count }})</button>
            </div>
            <span v-if="copyMessage" class="text-[13px] text-[#22c55e] font-medium">{{ copyMessage }}</span>
          </div>
          <div class="flex items-center gap-2">
            <button v-if="selectedUrls.size > 0" @click="batchAddToDownload" class="px-3 py-1 border-none rounded-md text-xs font-medium cursor-pointer transition-all bg-[#22c55e] text-white hover:bg-[#16a34a]">
              下载选中 ({{ selectedUrls.size }})
            </button>
            <button v-if="selectedUrls.size > 0" @click="batchCopy" class="px-3 py-1 bg-transparent text-[#667eea] border border-[#667eea] rounded-md text-xs cursor-pointer transition-all hover:bg-[#667eea] hover:text-white">
              复制选中
            </button>
          </div>
        </div>

        <!-- 历史记录工具栏 -->
        <div v-if="showHistory" class="flex justify-between items-center px-5 py-3 bg-[#fafbfc] dark:bg-gray-800 border-b border-[#f0f0f0] dark:border-gray-700 shrink-0">
          <span class="text-sm font-semibold text-[#1a1a2e] dark:text-gray-200">嗅探历史 ({{ dbRecords.length }} 条)</span>
          <button v-if="dbRecords.length > 0" @click="clearHistory" class="px-3 py-1 bg-transparent text-red-500 border border-red-300 rounded-md text-xs cursor-pointer transition-all hover:bg-red-500 hover:text-white">清空历史</button>
        </div>

        <!-- 媒体列表 -->
        <div class="flex-1 overflow-y-auto">
          <!-- 历史记录视图 -->
          <template v-if="showHistory">
            <div v-if="groupedHistory.length === 0" class="flex-1 flex flex-col items-center justify-center py-20 text-[#94a3b8]">
              <p class="text-sm">暂无嗅探历史</p>
            </div>
            <div v-for="group in groupedHistory" :key="group.page_url" class="border-b border-[#f0f0f0] dark:border-gray-800">
              <div class="flex items-center justify-between px-5 py-2.5 bg-[#f8f9fa] dark:bg-gray-800/50 cursor-pointer hover:bg-[#f0f0f0] dark:hover:bg-gray-700" @click="loadFromHistory(group)">
                <div class="flex-1 min-w-0">
                  <span class="block text-[13px] font-medium text-[#1a1a2e] dark:text-gray-200 whitespace-nowrap overflow-hidden text-ellipsis">{{ group.page_title }}</span>
                  <span class="block text-[11px] text-[#94a3b8] mt-0.5">{{ group.count }} 个资源 · {{ formatTime(group.time) }}</span>
                </div>
                <span class="text-xs text-[#667eea] ml-2">重新嗅探 →</span>
              </div>
              <div v-for="record in group.records.slice(0, 5)" :key="record.id" class="flex items-center px-5 py-2 pl-8 border-t border-[#f5f5f5] dark:border-gray-800">
                <span :class="['inline-block w-[50px] shrink-0 px-1.5 py-0.5 rounded text-[10px] font-bold uppercase text-center', getTypeColor(record.media_type)]">{{ record.file_ext || record.media_type }}</span>
                <span class="flex-1 min-w-0 px-3 text-[12px] font-mono text-[#1a1a2e] dark:text-gray-300 whitespace-nowrap overflow-hidden text-ellipsis" :title="record.url">{{ truncateUrl(record.url, 60) }}</span>
                <div class="flex items-center gap-1 shrink-0">
                  <IconButton variant="play" title="播放" @click.stop="playMedia(record.url, group.page_title)" />
                  <IconButton variant="download" title="添加到下载" @click.stop="addToDownload({ url: record.url, media_type: record.media_type, file_ext: record.file_ext, size: record.size, source: record.source })" />
                  <button @click.stop="copyUrl(record.url)" class="inline-flex items-center justify-center w-[28px] h-[28px] border border-[#dbe4f0] dark:border-gray-600 rounded-[9px] bg-[linear-gradient(180deg,#ffffff_0%,#f8fafc_100%)] dark:bg-gray-700 text-[#475569] dark:text-gray-400 cursor-pointer transition-all hover:-translate-y-px hover:border-[#b7c7dc]" title="复制链接">
                    <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
                  </button>
                </div>
              </div>
              <div v-if="group.records.length > 5" class="px-5 py-1.5 pl-8 text-[11px] text-[#94a3b8]">
                还有 {{ group.records.length - 5 }} 条...
              </div>
            </div>
          </template>

          <!-- 空状态 -->
          <div v-else-if="!sniffResult && !isSniffing" class="flex-1 flex flex-col items-center justify-center py-20 text-[#94a3b8]">
            <svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mb-4 text-[#cbd5e1] dark:text-gray-600">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
              <path d="M8 11h6" />
              <path d="M11 8v6" />
            </svg>
            <p class="text-sm mb-2">输入网页 URL 开始嗅探</p>
            <p class="text-xs text-[#cbd5e1] dark:text-gray-600">自动检测页面中的 m3u8、mp4、flv 等媒体资源</p>
          </div>

          <!-- 加载中 -->
          <div v-else-if="isSniffing" class="flex-1 flex flex-col items-center justify-center py-20">
            <div class="h-10 w-10 animate-spin rounded-full border-[3px] border-[#e5e7eb] dark:border-gray-600 border-t-[#f59e0b] mb-4"></div>
            <p class="text-sm text-[#64748b] dark:text-gray-400">正在嗅探页面资源...</p>
            <div v-if="logs.length > 0" class="mt-4 max-w-md">
              <p v-for="(log, i) in logs.slice(-3)" :key="i" class="text-xs text-[#94a3b8] mb-1">{{ log }}</p>
            </div>
          </div>

          <!-- 嗅探失败 -->
          <div v-else-if="sniffResult && !sniffResult.success" class="flex-1 flex flex-col items-center justify-center py-20 text-[#94a3b8]">
            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="mb-4 text-red-400">
              <circle cx="12" cy="12" r="10" />
              <line x1="15" y1="9" x2="9" y2="15" />
              <line x1="9" y1="9" x2="15" y2="15" />
            </svg>
            <p class="text-sm text-red-500">{{ sniffResult.message }}</p>
          </div>

          <!-- 无结果 -->
          <div v-else-if="sniffResult && sniffResult.media_list.length === 0" class="flex-1 flex flex-col items-center justify-center py-20 text-[#94a3b8]">
            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="mb-4 text-[#cbd5e1]">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <p class="text-sm">未发现媒体资源</p>
            <p class="text-xs mt-2 text-[#cbd5e1]">该页面可能没有可嗅探的视频/音频</p>
          </div>

          <!-- 结果列表 -->
          <template v-else-if="sniffResult && !showHistory">
            <div class="flex px-5 py-2.5 bg-[#f8f9fa] dark:bg-gray-800/50 border-b border-[#eee] dark:border-gray-700 text-xs font-semibold text-[#64748b] dark:text-gray-400 items-center">
              <div class="w-8 shrink-0">
                <input
                  type="checkbox"
                  :checked="selectedUrls.size > 0 && selectedUrls.size === filteredMedia.length"
                  :indeterminate="selectedUrls.size > 0 && selectedUrls.size < filteredMedia.length"
                  @change="toggleSelectAll"
                  class="w-3.5 h-3.5 cursor-pointer"
                />
              </div>
              <span class="w-[60px] shrink-0">类型</span>
              <span class="flex-1 min-w-0">URL</span>
              <span class="w-[70px] shrink-0 text-center">大小</span>
              <span class="w-[70px] shrink-0 text-center">来源</span>
              <span class="w-[120px] shrink-0">操作</span>
            </div>

            <div
              v-for="media in filteredMedia"
              :key="media.url"
              class="flex items-center px-5 py-2.5 border-b border-[#f5f5f5] dark:border-gray-800 transition-colors hover:bg-[#fafbfc] dark:hover:bg-gray-800/50"
              :class="{ 'bg-[#eff6ff] dark:bg-blue-900/10': selectedUrls.has(media.url) }"
            >
              <div class="w-8 shrink-0">
                <input
                  type="checkbox"
                  :checked="selectedUrls.has(media.url)"
                  @change="toggleSelect(media.url)"
                  class="w-3.5 h-3.5 cursor-pointer"
                />
              </div>
              <div class="w-[60px] shrink-0">
                <span :class="['inline-block px-2 py-0.5 rounded text-[10px] font-bold uppercase', getTypeColor(media.media_type)]">
                  {{ media.file_ext || media.media_type }}
                </span>
              </div>
              <div class="flex-1 min-w-0 pr-3">
                <span class="block text-[12px] font-mono text-[#1a1a2e] dark:text-gray-300 whitespace-nowrap overflow-hidden text-ellipsis" :title="media.url">
                  {{ truncateUrl(media.url) }}
                </span>
              </div>
              <div class="w-[70px] shrink-0 text-center text-[12px] text-[#64748b] dark:text-gray-400">
                {{ formatSize(media.size) }}
              </div>
              <div class="w-[70px] shrink-0 text-center">
                <span class="text-[10px] text-[#94a3b8] bg-[#f1f5f9] dark:bg-gray-800 px-1.5 py-0.5 rounded">
                  {{ getSourceLabel(media.source) }}
                </span>
              </div>
              <div class="w-[120px] flex items-center gap-1 shrink-0">
                <IconButton variant="play" title="播放" @click="playMedia(media.url, sniffResult?.page_title)" />
                <IconButton variant="download" title="添加到下载" @click="addToDownload(media)" />
                <button
                  @click="copyUrl(media.url)"
                  class="inline-flex items-center justify-center w-[30px] h-[30px] border border-[#dbe4f0] dark:border-gray-600 rounded-[9px] bg-[linear-gradient(180deg,#ffffff_0%,#f8fafc_100%)] dark:bg-gray-700 text-[#475569] dark:text-gray-400 cursor-pointer transition-all hover:-translate-y-px hover:border-[#b7c7dc] hover:shadow-[0_4px_10px_rgba(15,23,42,0.1)]"
                  title="复制链接"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                  </svg>
                </button>
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- 消息提示 -->
    <Transition name="fade">
      <div v-if="copyMessage" class="fixed bottom-6 left-1/2 -translate-x-1/2 px-4 py-2 bg-[#1e293b] text-white text-sm rounded-lg shadow-lg z-[100]">{{ copyMessage }}</div>
    </Transition>

    <!-- 播放器 -->
    <VideoPlayer :visible="playerVisible" :src="playerSrc" :title="playerTitle" :playlist="[]" :current-index="0" video-id="" @close="handlePlayerClose" />
  </div>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
