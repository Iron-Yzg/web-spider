<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { VideoItem, ScrapeResult, DownloadProgress, PaginatedVideos, Website } from '../types'
import { VideoStatus } from '../types'
import LogPopup from './LogPopup.vue'
import VideoPlayer from './VideoPlayer.vue'

const videoId = ref('')
const isScraping = ref(false)
const scrapeResult = ref<ScrapeResult | null>(null)
const videos = ref<VideoItem[]>([])
const filteredVideos = ref<VideoItem[]>([])
const downloadProgress = ref<Record<string, DownloadProgress>>({})
const logPopupVisible = ref(false)
const logPopupRef = ref<InstanceType<typeof LogPopup> | null>(null)
const copyMessage = ref('')
const searchQuery = ref('')
const statusFilter = ref<VideoStatus | ''>('')
const selectedIds = ref<Set<string>>(new Set())
const confirmDialog = ref<{ visible: boolean, message: string, onConfirm: (() => void) | null }>({
  visible: false,
  message: '',
  onConfirm: null
})

// 封面浮窗状态
const coverPopupVisible = ref(false)
const coverPopupImage = ref('')
const coverPopupPosition = ref({ x: 0, y: 0 })

// 显示封面浮窗
function showCoverPopup(event: MouseEvent, imageUrl: string) {
  coverPopupImage.value = imageUrl
  coverPopupPosition.value = { x: event.clientX, y: event.clientY }
  coverPopupVisible.value = true
}

// 隐藏封面浮窗
function hideCoverPopup() {
  coverPopupVisible.value = false
  coverPopupImage.value = ''
}

// 移动封面浮窗
function moveCoverPopup(event: MouseEvent) {
  if (coverPopupVisible.value) {
    coverPopupPosition.value = { x: event.clientX, y: event.clientY }
  }
}

// 网站选择
const websites = ref<Website[]>([])
const selectedWebsite = ref<string>('')

// 获取当前选中网站的名称
const selectedWebsiteName = ref<string>('')

function updateSelectedWebsiteName() {
  const website = websites.value.find(w => w.id === selectedWebsite.value)
  selectedWebsiteName.value = website?.name || ''
}

// 分页状态
const currentPage = ref(1)
const pageSize = 20
const total = ref(0)
const hasMore = ref(false)
const isLoadingMore = ref(false)

// 播放器状态
const playerVisible = ref(false)
const playerSrc = ref('')
const playerTitle = ref('')
const playerPlaylist = ref<VideoItem[]>([])
const currentVideoIndex = ref(0)

let unlistenVideos: (() => void) | null = null
let unlistenProgress: (() => void) | null = null
let unlistenScrapeLog: (() => void) | null = null

// 监听搜索和筛选变化，重置列表
watch([searchQuery, statusFilter], async () => {
  currentPage.value = 1
  videos.value = []
  if (searchQuery.value.trim()) {
    await searchVideos()
  } else {
    await loadVideos()
  }
})

// 监听网站选择变化，重置列表
watch(selectedWebsite, async () => {
  updateSelectedWebsiteName()
  currentPage.value = 1
  videos.value = []
  await loadVideos()
})

onMounted(async () => {
  await Promise.all([
    loadVideos(),
    loadWebsites()
  ])

  unlistenVideos = await listen<VideoItem[]>('videos-updated', async () => {
    // 重新加载第一页
    currentPage.value = 1
    videos.value = []
    await loadVideos()
  })

  unlistenProgress = await listen<DownloadProgress>('event', (event) => {
    const progress = event.payload
    downloadProgress.value[progress.video_id] = progress

    // 根据进度状态更新视频列表中的状态
    const video = videos.value.find(v => v.id === progress.video_id)
    if (video) {
      if (progress.status === '下载完成' || progress.progress >= 100) {
        video.status = VideoStatus.Downloaded
      } else if (progress.status.startsWith('下载失败')) {
        video.status = VideoStatus.Scraped
      } else if (progress.progress > 0) {
        video.status = VideoStatus.Downloading
      }
    }
  })

  unlistenScrapeLog = await listen<string>('scrape-log', (event) => {
    if (logPopupRef.value) {
      logPopupRef.value.addLog(event.payload)
    }
  })
})

onUnmounted(() => {
  if (unlistenVideos) unlistenVideos()
  if (unlistenProgress) unlistenProgress()
  if (unlistenScrapeLog) unlistenScrapeLog()
})

async function loadVideos(isLoadMore = false) {
  try {
    if (!isLoadMore) {
      // 首次加载，显示加载状态
      isLoadingMore.value = true
    }

    let result: PaginatedVideos

    // 如果选择了网站，按网站名称筛选
    if (selectedWebsiteName.value) {
      result = await invoke<PaginatedVideos>('get_videos_by_website', {
        websiteName: selectedWebsiteName.value,
        page: currentPage.value,
        pageSize
      })
    } else {
      result = await invoke<PaginatedVideos>('get_videos_paginated', {
        page: currentPage.value,
        pageSize
      })
    }

    if (isLoadMore) {
      videos.value = [...videos.value, ...result.videos]
    } else {
      videos.value = result.videos
    }

    total.value = result.total
    hasMore.value = result.has_more
    filterVideos()
  } catch (e) {
    console.error('加载视频列表失败:', e)
  } finally {
    isLoadingMore.value = false
  }
}

async function loadWebsites() {
  try {
    websites.value = await invoke<Website[]>('get_websites')
    // 自动选择默认网站
    const defaultSite = websites.value.find(w => w.is_default)
    if (defaultSite) {
      selectedWebsite.value = defaultSite.id
    } else if (websites.value.length > 0) {
      selectedWebsite.value = websites.value[0].id
    }
    updateSelectedWebsiteName()
  } catch (e) {
    console.error('加载网站列表失败:', e)
  }
}

async function loadMore() {
  if (isLoadingMore.value || !hasMore.value) return

  isLoadingMore.value = true
  currentPage.value++
  await loadVideos(true)
  isLoadingMore.value = false
}

// 滚动到底部自动加载更多
function handleScroll(e: Event) {
  const target = e.target as HTMLElement
  const { scrollTop, scrollHeight, clientHeight } = target

  // 当滚动到距离底部 50px 以内时触发加载
  if (scrollHeight - scrollTop - clientHeight < 50) {
    loadMore()
  }
}

async function searchVideos() {
  if (!searchQuery.value.trim()) {
    await loadVideos()
    return
  }

  try {
    isLoadingMore.value = true
    const result = await invoke<PaginatedVideos>('search_videos', {
      query: searchQuery.value,
      page: currentPage.value,
      pageSize
    })

    videos.value = result.videos
    total.value = result.total
    hasMore.value = result.has_more
    filterVideos()
  } catch (e) {
    console.error('搜索视频失败:', e)
  } finally {
    isLoadingMore.value = false
  }
}

function filterVideos() {
  let result = videos.value

  // 按状态筛选
  if (statusFilter.value) {
    result = result.filter(v => v.status === statusFilter.value)
  }

  filteredVideos.value = result
}

async function scrape() {
  if (websites.value.length === 0) {
    alert('请先在设置页面添加网站配置')
    return
  }

  if (!videoId.value.trim()) {
    alert('请输入视频ID')
    return
  }

  // 清空之前的结果
  scrapeResult.value = null

  // 显示日志弹窗
  logPopupVisible.value = true

  // 延迟一点确保组件已渲染
  await nextTick()
  if (logPopupRef.value) {
    const website = websites.value.find(w => w.id === selectedWebsite.value)
    logPopupRef.value.addLog(`开始爬取视频: ${videoId.value.trim()} (${website?.name || '未知网站'})`)
  }

  isScraping.value = true

  try {
    const result = await invoke<ScrapeResult>('scrape_video', {
      videoId: videoId.value.trim(),
      websiteId: selectedWebsite.value || null
    })

    scrapeResult.value = result

    if (result.success) {
      if (logPopupRef.value) {
        logPopupRef.value.addLog(`爬取成功: ${result.name}`)
        logPopupRef.value.addLog(`m3u8地址: ${result.m3u8_url}`)
      }
      await loadVideos()
      videoId.value = ''

      // 成功后自动关闭弹窗
      // setTimeout(() => {
      //   logPopupVisible.value = false
      // }, 1500)
    } else {
      if (logPopupRef.value) {
        logPopupRef.value.addLog(`爬取失败: ${result.message}`)
      }
    }
  } catch (e) {
    const errorMsg = '爬取失败: ' + e
    scrapeResult.value = {
      success: false,
      name: '',
      m3u8_url: '',
      message: errorMsg
    }
    if (logPopupRef.value) {
      logPopupRef.value.addLog(`${errorMsg}`)
    }
  } finally {
    isScraping.value = false
  }
}

function handleLogPopupClose() {
  if (isScraping.value) {
    if (confirm('爬取尚未完成，确定要关闭日志窗口吗？')) {
      logPopupVisible.value = false
    }
  } else {
    logPopupVisible.value = false
  }
}

async function downloadVideo(video: VideoItem) {

  try {
    await invoke('download_video', { videoId: video.id })
    await loadVideos()
  } catch (e) {
    alert('下载失败: ' + e)
  }
}

async function batchDownload() {
  const ids = Array.from(selectedIds.value)
  if (ids.length === 0) {
    alert('请先选择要下载的视频')
    return
  }

  try {
    await invoke('batch_download', { videoIds: ids })
    selectedIds.value.clear()
    await loadVideos()
  } catch (e) {
    alert('批量下载失败: ' + e)
  }
}

async function deleteVideo(id: string) {
  // 使用自定义确认对话框
  confirmDialog.value = {
    visible: true,
    message: '确定要删除这个视频吗？',
    onConfirm: async () => {
      try {
        await invoke('delete_video', { videoId: id })
        selectedIds.value.delete(id)
        await loadVideos()
      } catch (e) {
        alert('删除失败: ' + e)
      }
    }
  }
}

async function deleteSelected() {
  const ids = Array.from(selectedIds.value)
  if (ids.length === 0) {
    alert('请先选择要删除的视频')
    return
  }

  confirmDialog.value = {
    visible: true,
    message: `确定要删除选中的 ${ids.length} 个视频吗？`,
    onConfirm: async () => {
      try {
        for (const id of ids) {
          await invoke('delete_video', { videoId: id })
        }
        selectedIds.value.clear()
        await loadVideos()
      } catch (e) {
        alert('批量删除失败: ' + e)
      }
    }
  }
}

async function copyUrl(url: string) {
  try {
    await navigator.clipboard.writeText(url)
    copyMessage.value = '已复制到剪贴板'
    setTimeout(() => {
      copyMessage.value = ''
    }, 2000)
  } catch (e) {
    console.error('复制失败:', e)
  }
}

async function clearDownloaded() {
  // 使用自定义确认对话框
  confirmDialog.value = {
    visible: true,
    message: '确定要清除所有已下载的视频吗？',
    onConfirm: async () => {
      try {
        await invoke('clear_downloaded')
        await loadVideos()
      } catch (e) {
        alert('清除失败: ' + e)
      }
    }
  }
}

function handleConfirm() {
  if (confirmDialog.value.onConfirm) {
    confirmDialog.value.onConfirm()
  }
  confirmDialog.value.visible = false
  confirmDialog.value.onConfirm = null
}

function handleCancel() {
  confirmDialog.value.visible = false
  confirmDialog.value.onConfirm = null
}

function toggleSelectAll(e: Event) {
  const checked = (e.target as HTMLInputElement).checked
  if (checked) {
    // 只选中可下载的视频
    filteredVideos.value.forEach(v => {
      if (v.status !== VideoStatus.Downloaded && v.status !== VideoStatus.Downloading) {
        selectedIds.value.add(v.id)
      }
    })
  } else {
    selectedIds.value.clear()
  }
}

function toggleSelect(id: string) {
  if (selectedIds.value.has(id)) {
    selectedIds.value.delete(id)
  } else {
    selectedIds.value.add(id)
  }
}

function getStatusText(status: VideoStatus): string {
  const map: Record<VideoStatus, string> = {
    [VideoStatus.Pending]: '待爬取',
    [VideoStatus.Scraped]: '已爬取',
    [VideoStatus.Downloading]: '下载中',
    [VideoStatus.Downloaded]: '已下载',
    [VideoStatus.Failed]: '失败'
  }
  return map[status] || status
}

function getStatusClass(status: VideoStatus): string {
  const map: Record<VideoStatus, string> = {
    [VideoStatus.Pending]: 'status-pending',
    [VideoStatus.Scraped]: 'status-scraped',
    [VideoStatus.Downloading]: 'status-downloading',
    [VideoStatus.Downloaded]: 'status-downloaded',
    [VideoStatus.Failed]: 'status-failed'
  }
  return map[status] || ''
}

function getProgress(progress: DownloadProgress | undefined): number {
  return progress?.progress || 0
}

function isDownloading(video: VideoItem): boolean {
  return video.status === VideoStatus.Downloading && !!downloadProgress.value[video.id]
}

// 打开播放器
function openPlayer(video: VideoItem) {
  playerSrc.value = video.m3u8_url
  playerTitle.value = video.name
  // 设置播放列表和当前索引
  playerPlaylist.value = videos.value
  currentVideoIndex.value = videos.value.findIndex(v => v.id === video.id)
  playerVisible.value = true
}

// 处理播放下一个视频
async function handlePlayNext(nextIndex: number) {
  if (nextIndex >= 0 && nextIndex < playerPlaylist.value.length) {
    const nextVideo = playerPlaylist.value[nextIndex]
    playerSrc.value = nextVideo.m3u8_url
    playerTitle.value = nextVideo.name
    currentVideoIndex.value = nextIndex
  }
}

// 处理删除当前视频
async function handleDeleteCurrent() {
  const currentVideo = playerPlaylist.value[currentVideoIndex.value]
  if (!currentVideo) return

  try {
    // 调用删除接口
    await invoke('delete_video', { videoId: currentVideo.id })
    // 从本地列表中移除
    const index = videos.value.findIndex(v => v.id === currentVideo.id)
    if (index > -1) {
      videos.value.splice(index, 1)
      total.value--
    }
    // 从播放列表中移除
    playerPlaylist.value.splice(currentVideoIndex.value, 1)

    // 如果还有视频，播放下一个
    if (playerPlaylist.value.length > 0) {
      // 如果删除的是最后一个，回到前一个
      const nextIndex = currentVideoIndex.value >= playerPlaylist.value.length
        ? playerPlaylist.value.length - 1
        : currentVideoIndex.value
      const nextVideo = playerPlaylist.value[nextIndex]
      playerSrc.value = nextVideo.m3u8_url
      playerTitle.value = nextVideo.name
      currentVideoIndex.value = nextIndex
    } else {
      // 没有视频了，关闭播放器
      handlePlayerClose()
    }
  } catch (e) {
    console.error('删除视频失败:', e)
  }
}

// 关闭播放器
function handlePlayerClose() {
  playerVisible.value = false
  playerSrc.value = ''
  playerTitle.value = ''
  playerPlaylist.value = []
  currentVideoIndex.value = 0
}

// 格式化数字（万、亿）
function formatCount(count: number | null | undefined): string {
  if (count === null || count === undefined) return '-'
  if (count >= 100000000) {
    return (count / 100000000).toFixed(1) + '亿'
  }
  if (count >= 10000) {
    return (count / 10000).toFixed(1) + '万'
  }
  return count.toString()
}

// 处理图片加载失败
function handleImageError(event: Event) {
  const img = event.target as HTMLImageElement
  img.style.display = 'none'
  // 显示占位符
  const parent = img.parentElement
  if (parent) {
    const placeholder = parent.querySelector('.cover-placeholder-small') as HTMLElement
    if (placeholder) {
      placeholder.style.display = 'flex'
    }
  }
}
</script>

<template>
  <div class="scraper-page">
    <!-- 顶部搜索栏 -->
    <div class="search-bar">
      <!-- 网站选择 -->
      <select v-model="selectedWebsite" :disabled="isScraping" class="website-select">
        <option v-for="site in websites" :key="site.id" :value="site.id">
          {{ site.name }}
        </option>
      </select>
      <input
        type="text"
        v-model="videoId"
        placeholder="输入视频ID"
        @keyup.enter="scrape"
        :disabled="isScraping"
        class="search-input"
      />
      <button @click="scrape" :disabled="isScraping" class="search-btn">
        {{ isScraping ? '爬取中...' : '爬取' }}
      </button>
    </div>

    <!-- 爬取结果提示 -->
    <div v-if="scrapeResult" :class="['result-toast', scrapeResult.success ? 'success' : 'error']">
      <span v-if="scrapeResult.success"> {{ scrapeResult.name }}</span>
      <span v-else> {{ scrapeResult.message }}</span>
    </div>

    <!-- 视频列表 -->
    <div class="video-section">
      <div class="section-header">
        <div class="header-left">
          <span class="section-title">视频列表 ({{ filteredVideos.length }}/{{ videos.length }})</span>
          <span v-if="copyMessage" class="copy-success">{{ copyMessage }}</span>
        </div>
        <div class="header-right">
          <!-- 搜索框 -->
          <input
            type="text"
            v-model="searchQuery"
            @input="filterVideos"
            placeholder="搜索视频名称"
            class="filter-input"
          />
          <!-- 状态筛选 -->
          <select v-model="statusFilter" @change="filterVideos" class="filter-select">
            <option value="">全部状态</option>
            <option :value="VideoStatus.Pending">待爬取</option>
            <option :value="VideoStatus.Scraped">已爬取</option>
            <option :value="VideoStatus.Downloading">下载中</option>
            <option :value="VideoStatus.Downloaded">已下载</option>
            <option :value="VideoStatus.Failed">失败</option>
          </select>
          <!-- 批量操作按钮 -->
          <button
            v-if="selectedIds.size > 0"
            @click="batchDownload"
            class="batch-btn batch-download"
          >
            下载选中 ({{ selectedIds.size }})
          </button>
          <button
            v-if="selectedIds.size > 0"
            @click="deleteSelected"
            class="batch-btn batch-delete"
          >
            删除选中
          </button>
          <button
            v-if="videos.some(v => v.status === VideoStatus.Downloaded)"
            @click="clearDownloaded"
            class="clear-btn"
          >
            清除已下载
          </button>
        </div>
      </div>

      <div class="video-table">
        <div class="table-header">
          <div class="col-checkbox">
            <input
              type="checkbox"
              @change="toggleSelectAll"
              :checked="filteredVideos.length > 0 && filteredVideos.every(v => selectedIds.has(v.id) || v.status === VideoStatus.Downloaded || v.status === VideoStatus.Downloading)"
              :indeterminate="selectedIds.size > 0 && !filteredVideos.every(v => selectedIds.has(v.id) || v.status === VideoStatus.Downloaded || v.status === VideoStatus.Downloading)"
            />
          </div>
          <span class="col-name">名称</span>
          <span class="col-status">状态</span>
          <span class="col-action">操作</span>
        </div>

        <div
        class="table-body"
        @scroll="handleScroll"
        ref="tableBodyRef"
      >
          <div v-if="videos.length === 0 && !isLoadingMore" class="empty-tip">
            输入视频ID开始爬取
          </div>
          <div v-else-if="filteredVideos.length === 0 && !isLoadingMore" class="empty-tip">
            没有找到匹配的视频
          </div>
          <div v-else-if="isLoadingMore && videos.length === 0" class="empty-tip">
            加载中...
          </div>

          <div v-for="video in filteredVideos" :key="video.id" class="table-row">
            <div class="col-checkbox">
              <input
                type="checkbox"
                :checked="selectedIds.has(video.id)"
                :disabled="video.status === VideoStatus.Downloaded || video.status === VideoStatus.Downloading"
                @change="toggleSelect(video.id)"
              />
            </div>
            <!-- 封面图片 -->
            <div class="col-cover" @click="openPlayer(video)">
              <img
                v-if="video.cover_url"
                :src="video.cover_url"
                :alt="video.name"
                class="cover-thumbnail"
                @error="handleImageError"
                @mouseenter="showCoverPopup($event, video.cover_url)"
                @mousemove="moveCoverPopup($event)"
                @mouseleave="hideCoverPopup"
              />
              <div v-else class="cover-placeholder-small">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"></rect>
                  <line x1="7" y1="2" x2="7" y2="22"></line>
                  <line x1="17" y1="2" x2="17" y2="22"></line>
                  <line x1="2" y1="12" x2="22" y2="12"></line>
                </svg>
              </div>
            </div>
            <div class="col-name">
              <span class="video-name" :title="video.name">{{ video.name }}</span>
              <div class="video-tags">
                <span v-if="video.view_count !== undefined && video.view_count !== null" class="tag tag-views">
                  <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polygon points="5 3 19 12 5 21 5 3"></polygon>
                  </svg>
                  {{ formatCount(video.view_count) }}
                </span>
                <span v-if="video.favorite_count !== undefined && video.favorite_count !== null" class="tag tag-favorites">
                  <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"></path>
                  </svg>
                  {{ formatCount(video.favorite_count) }}
                </span>
              </div>
            </div>
            <div class="col-status">
              <!-- 下载中显示进度条 -->
              <div v-if="isDownloading(video)" class="xunlei-progress">
                <div class="xunlei-progress-bar">
                  <div
                    class="xunlei-progress-fill"
                    :style="{ width: getProgress(downloadProgress[video.id]) + '%' }"
                  ></div>
                </div>
                <div class="xunlei-progress-info">
                  <span class="xunlei-percent">{{ Math.round(getProgress(downloadProgress[video.id])) }}%</span>
                  <span class="xunlei-speed">{{ downloadProgress[video.id]?.speed || '0 MB/s' }}</span>
                </div>
              </div>
              <!-- 非下载状态显示状态标签 -->
              <span v-else :class="['status-tag', getStatusClass(video.status)]">
                {{ getStatusText(video.status) }}
              </span>
            </div>
            <div class="col-action">
              <!-- 播放按钮（仅已爬取或已下载的视频可播放） -->
              <button
                @click="openPlayer(video)"
                class="action-btn play"
                title="播放"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="none">
                  <polygon points="5 3 19 12 5 21 5 3"></polygon>
                </svg>
              </button>
              <!-- 复制按钮 -->
              <button
                @click="copyUrl(video.m3u8_url)"
                class="action-btn copy"
                title="复制链接"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>
              </button>
              <!-- 下载按钮（下载中时不显示，下载完成后显示已完成） -->
              <button
                @click="downloadVideo(video)"
                class="action-btn download"
                title="下载"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
                  <polyline points="7 10 12 15 17 10"></polyline>
                  <line x1="12" y1="15" x2="12" y2="3"></line>
                </svg>
              </button>
              <!-- 删除按钮 -->
              <button @click="deleteVideo(video.id)" class="action-btn delete" title="删除">
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18"></line>
                  <line x1="6" y1="6" x2="18" y2="18"></line>
                </svg>
              </button>
            </div>
          </div>

          <!-- 加载更多 -->
          <div v-if="hasMore || isLoadingMore" class="load-more">
            <button
              v-if="hasMore && !isLoadingMore"
              @click="loadMore"
              class="load-more-btn"
            >
              加载更多
            </button>
            <span v-else-if="isLoadingMore" class="loading-text">加载中...</span>
          </div>

          <!-- 底部统计 -->
          <div v-if="videos.length > 0" class="pagination-info">
            共 {{ total }} 条视频，当前显示 {{ videos.length }} 条
          </div>
        </div>
      </div>
    </div>

    <!-- 日志弹窗 -->
    <LogPopup
      ref="logPopupRef"
      :visible="logPopupVisible"
      title="爬取日志"
      @close="handleLogPopupClose"
    />

    <!-- 确认对话框 -->
    <Teleport to="body">
      <div v-if="confirmDialog.visible" class="confirm-overlay" @click="handleCancel">
        <div class="confirm-dialog" @click.stop>
          <div class="confirm-content">{{ confirmDialog.message }}</div>
          <div class="confirm-actions">
            <button class="confirm-btn cancel" @click="handleCancel">取消</button>
            <button class="confirm-btn ok" @click="handleConfirm">确定</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- 封面浮窗（放大2倍显示） -->
    <Teleport to="body">
      <div
        v-if="coverPopupVisible"
        class="cover-popup"
        :style="{
          left: coverPopupPosition.x + 20 + 'px',
          top: coverPopupPosition.y - 150 + 'px'
        }"
        @mouseleave="hideCoverPopup"
      >
        <img
          :src="coverPopupImage"
          alt="封面预览"
          class="cover-popup-image"
        />
      </div>
    </Teleport>

    <!-- 视频播放器 -->
    <VideoPlayer
      :visible="playerVisible"
      :src="playerSrc"
      :title="playerTitle"
      :playlist="playerPlaylist"
      :current-index="currentVideoIndex"
      @close="handlePlayerClose"
      @play-next="handlePlayNext"
      @delete-current="handleDeleteCurrent"
    />
  </div>
</template>

<style scoped>
.scraper-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: #fff;
  border-radius: 12px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.06);
  overflow: hidden;
}

/* 搜索栏 */
.search-bar {
  display: flex;
  gap: 10px;
  padding: 16px 20px;
  border-bottom: 1px solid #f0f0f0;
  flex-shrink: 0;
}

.website-select {
  padding: 10px 14px;
  border: 1px solid #e8e8e8;
  border-radius: 8px;
  font-size: 14px;
  background: white;
  cursor: pointer;
  min-width: 140px;
  transition: all 0.2s;
}

.website-select:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.website-select:disabled {
  opacity: 0.6;
  cursor: not-allowed;
  background: #fafbfc;
}

.search-input {
  flex: 1;
  padding: 10px 14px;
  border: 1px solid #e8e8e8;
  border-radius: 8px;
  font-size: 14px;
  transition: all 0.2s;
}

.search-input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.search-btn {
  padding: 10px 24px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  white-space: nowrap;
}

.search-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.search-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* 结果提示 */
.result-toast {
  margin: 0 20px;
  padding: 10px 14px;
  border-radius: 8px;
  font-size: 13px;
  flex-shrink: 0;
}

.result-toast.success {
  background: #f0fdf4;
  color: #166534;
  border: 1px solid #bbf7d0;
}

.result-toast.error {
  background: #fef2f2;
  color: #991b1b;
  border: 1px solid #fecaca;
}

/* 视频区域 */
.video-section {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  background: #fafbfc;
  border-bottom: 1px solid #f0f0f0;
  flex-shrink: 0;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  color: #1a1a2e;
}

.filter-input {
  padding: 6px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 6px;
  font-size: 13px;
  width: 180px;
  transition: all 0.2s;
}

.filter-input:focus {
  outline: none;
  border-color: #667eea;
}

.filter-select {
  padding: 6px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 6px;
  font-size: 13px;
  background: white;
  cursor: pointer;
  transition: all 0.2s;
}

.filter-select:focus {
  outline: none;
  border-color: #667eea;
}

.batch-btn {
  padding: 6px 14px;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.batch-download {
  background: #22c55e;
  color: white;
}

.batch-download:hover {
  background: #16a34a;
}

.batch-delete {
  background: #fee2e2;
  color: #dc2626;
}

.batch-delete:hover {
  background: #fecaca;
}

.clear-btn {
  padding: 4px 12px;
  background: transparent;
  color: #667eea;
  border: 1px solid #667eea;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.clear-btn:hover {
  background: #667eea;
  color: white;
}

.copy-success {
  color: #22c55e;
  font-size: 13px;
  font-weight: 500;
  animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(-5px); }
  to { opacity: 1; transform: translateY(0); }
}

/* 表格 */
.video-table {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.table-header {
  display: flex;
  padding: 10px 20px;
  background: #f8f9fa;
  border-bottom: 1px solid #eee;
  font-size: 12px;
  font-weight: 600;
  color: #64748b;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  align-items: center;
}

.table-body {
  flex: 1;
  overflow-y: auto;
}

.empty-tip {
  padding: 40px 20px;
  text-align: center;
  color: #94a3b8;
  font-size: 13px;
}

.table-row {
  display: flex;
  align-items: center;
  padding: 12px 20px;
  border-bottom: 1px solid #f5f5f5;
  transition: background 0.15s;
}

.table-row:hover {
  background: #fafbfc;
}

.col-checkbox {
  width: 32px;
  flex-shrink: 0;
}

.col-checkbox input {
  cursor: pointer;
  width: 16px;
  height: 16px;
}

/* 封面图片列 */
.col-cover {
  width: 60px;
  height: 34px;
  flex-shrink: 0;
  margin-right: 12px;
  cursor: pointer;
  position: relative;
  overflow: hidden;
  border-radius: 4px;
  background: #f5f5f5;
}

.cover-thumbnail {
  width: 100%;
  height: 100%;
  object-fit: cover;
  transition: transform 0.2s ease;
}

.col-cover:hover .cover-thumbnail {
  transform: scale(1.2);
}

.cover-placeholder-small {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f8f9fa;
  color: #cbd5e1;
}

/* 视频标签 */
.video-tags {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}

.tag {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
}

.tag-views {
  background: #f0f9ff;
  color: #0369a1;
}

.tag-favorites {
  background: #fef2f2;
  color: #dc2626;
}

.tag svg {
  flex-shrink: 0;
}

.col-name {
  flex: 1;
  min-width: 0;
  padding-right: 16px;
}

.video-name {
  display: block;
  font-size: 14px;
  font-weight: 500;
  color: #1a1a2e;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.video-url {
  display: block;
  font-size: 11px;
  color: #94a3b8;
  font-family: monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.col-status {
  width: 100px;
  padding-right: 16px;
}

.status-tag {
  display: inline-block;
  padding: 4px 10px;
  border-radius: 12px;
  font-size: 11px;
  font-weight: 500;
}

.status-pending { background: #fef3c7; color: #d97706; }
.status-scraped { background: #dbeafe; color: #2563eb; }
.status-downloading { background: #dcfce7; color: #16a34a; }
.status-downloaded { background: #dcfce7; color: #16a34a; }
.status-failed { background: #fee2e2; color: #dc2626; }

/* 迅雷风格进度条 */
.xunlei-progress {
  width: 100%;
}

.xunlei-progress-bar {
  height: 6px;
  background: #e5e7eb;
  border-radius: 3px;
  overflow: hidden;
  margin-bottom: 4px;
}

.xunlei-progress-fill {
  height: 100%;
  background: linear-gradient(90deg, #22c55e, #16a34a);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.xunlei-progress-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 10px;
}

.xunlei-percent {
  color: #16a34a;
  font-weight: 600;
}

.xunlei-speed {
  color: #64748b;
}

.col-action {
  width: 150px;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.action-btn {
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.action-btn.download {
  background: #22c55e;
  color: white;
}

.action-btn.download:hover {
  background: #16a34a;
}

.action-btn.copy {
  background: #e0e7ff;
  color: #4f46e5;
}

.action-btn.copy:hover {
  background: #c7d2fe;
}

.action-btn.play {
  background: #fef3c7;
  color: #d97706;
}

.action-btn.play:hover {
  background: #fde68a;
}

.action-btn.delete {
  width: 28px;
  height: 28px;
  padding: 0;
  background: #fee2e2;
  color: #dc2626;
}

.action-btn.delete:hover {
  background: #fecaca;
}

.done-tag {
  padding: 6px 12px;
  background: #f0fdf4;
  color: #16a34a;
  border-radius: 6px;
  font-size: 12px;
}

/* 加载更多 */
.load-more {
  padding: 16px;
  text-align: center;
}

.load-more-btn {
  padding: 8px 24px;
  background: #f1f5f9;
  color: #64748b;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.load-more-btn:hover {
  background: #e2e8f0;
  color: #475569;
}

.loading-text {
  color: #94a3b8;
  font-size: 13px;
}

.pagination-info {
  padding: 12px 20px;
  text-align: right;
  font-size: 12px;
  color: #94a3b8;
  border-top: 1px solid #f0f0f0;
}

/* 确认对话框 */
.confirm-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.confirm-dialog {
  background: white;
  border-radius: 12px;
  padding: 24px;
  min-width: 300px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
}

.confirm-content {
  font-size: 15px;
  color: #1a1a2e;
  margin-bottom: 20px;
  text-align: center;
}

.confirm-actions {
  display: flex;
  gap: 12px;
  justify-content: center;
}

.confirm-btn {
  padding: 8px 24px;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.confirm-btn.cancel {
  background: #f1f5f9;
  color: #64748b;
}

.confirm-btn.cancel:hover {
  background: #e2e8f0;
}

.confirm-btn.ok {
  background: #4f46e5;
  color: white;
}

.confirm-btn.ok:hover {
  background: #4338ca;
}

/* 封面浮窗 */
.cover-popup {
  position: fixed;
  z-index: 1000;
  padding: 8px;
  background: white;
  border-radius: 8px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.2);
  pointer-events: auto;
}

.cover-popup-image {
  width: 400px;
  height: auto;
  object-fit: contain;
  border-radius: 4px;
  display: block;
}
</style>
