<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import type { VideoItem, ScrapeResult, DownloadProgress, PaginatedVideos, Website, LocalVideo } from '../types'
import { VideoStatus } from '../types'
import VideoPlayer from '../components/VideoPlayer.vue'
import DlnaCastDialog from '../components/DlnaCastDialog.vue'
import IconButton from '../components/IconButton.vue'
import {
  getVideos as fetchVideos,
  searchVideos as searchVideosApi,
  getVideosByWebsite as fetchVideosByWebsite,
  scrapeVideo as scrapeVideoApi,
  deleteVideo as deleteVideoApi,
  batchDownload as batchDownloadApi,
  clearDownloadedVideos,
  getWebsites as fetchWebsites,
  getWebsiteByName as fetchWebsiteByName,
  downloadVideo as downloadVideoApi,
} from '../services/api'
import LogPopup from '../components/LogPopup.vue'

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
const playerVideoId = ref('')
const playerPlaylist = ref<VideoItem[]>([])
const currentVideoIndex = ref(0)

// DLNA 投屏状态
const showDlnaDialog = ref(false)
const dlnaVideo = ref<LocalVideo | null>(null)

function openDlnaDialog(video: VideoItem) {
  // 优先使用本地文件，其次使用网络地址
  if (!video.file_path && !video.m3u8_url) {
    return // 没有视频地址，无法投屏
  }
  dlnaVideo.value = {
    id: video.id,
    name: video.name,
    file_path: video.file_path || '',  // 本地文件优先
    m3u8_url: video.m3u8_url || '',   // 网络地址作为备选
    file_size: '',
    duration: '',
    resolution: '',
    added_at: '',
  }
  showDlnaDialog.value = true
}

function closeDlnaDialog() {
  showDlnaDialog.value = false
  dlnaVideo.value = null
}

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
  // 先加载网站列表，确保 selectedWebsiteName 有值
  await loadWebsites()
  // 再加载视频列表
  await loadVideos()

  unlistenVideos = await listen<VideoItem[]>('videos-updated', (event) => {
    const newVideos = event.payload
    // 保留正在下载的视频的进度状态
    const currentDownloadingIds = Object.keys(downloadProgress.value)

    // 按网站筛选：如果选择了网站，只保留该网站的视频
    let filteredByWebsite = newVideos
    if (selectedWebsiteName.value) {
      filteredByWebsite = newVideos.filter(v => v.website_name === selectedWebsiteName.value)
    }

    // 创建新数组，保留正在下载视频的本地状态
    const updatedVideos = filteredByWebsite.map(newVideo => {
      // 如果这个视频正在下载，检查本地状态
      if (currentDownloadingIds.includes(newVideo.id)) {
        const existingVideo = videos.value.find(v => v.id === newVideo.id)
        if (existingVideo) {
          // 保留本地状态（status），只更新其他字段
          return { ...newVideo, status: existingVideo.status }
        }
      }
      return newVideo
    })

    // 移除已完成下载的视频的进度信息
    for (const videoId of currentDownloadingIds) {
      const video = updatedVideos.find(v => v.id === videoId)
      if (video && (video.status === VideoStatus.Downloaded || video.status === VideoStatus.Scraped)) {
        // 下载已完成或失败，移除进度信息
        delete downloadProgress.value[videoId]
      }
    }

    // 更新视频列表
    videos.value = updatedVideos
    filterVideos()
  })

  unlistenProgress = await listen<DownloadProgress>('event', (event) => {
    const progress = event.payload
    downloadProgress.value[progress.video_id] = progress

    // 根据进度状态更新视频列表中的状态
    // 找到视频索引直接更新，触发Vue响应式更新
    const index = videos.value.findIndex(v => v.id === progress.video_id)
    if (index !== -1) {
      if (progress.status === '下载完成' || progress.progress >= 100) {
        videos.value[index].status = VideoStatus.Downloaded
        // 下载完成，移除进度信息
        delete downloadProgress.value[progress.video_id]
      } else if (progress.status.startsWith('下载失败')) {
        videos.value[index].status = VideoStatus.Scraped
        // 下载失败，移除进度信息
        delete downloadProgress.value[progress.video_id]
      } else if (progress.progress > 0) {
        videos.value[index].status = VideoStatus.Downloading
      }
      // 触发筛选器重新应用（如果需要）
      filterVideos()
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
      result = await fetchVideosByWebsite(
        selectedWebsiteName.value,
        currentPage.value,
        pageSize
      )
    } else {
      result = await fetchVideos(currentPage.value, pageSize)
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
    websites.value = await fetchWebsites()
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

// 从URL中提取token参数
function extractUrlToken(url: string): string | null {
  try {
    const parsed = new URL(url)
    const params = new URLSearchParams(parsed.search)
    return params.get('token')
  } catch {
    return null
  }
}

// 替换URL中的token
function replaceUrlToken(url: string, newToken: string): string {
  try {
    const parsed = new URL(url)
    const params = new URLSearchParams(parsed.search)
    params.set('token', newToken)
    parsed.search = params.toString()
    return parsed.toString()
  } catch {
    return url
  }
}

// 替换视频URL中的token（根据视频的website_name获取对应网站的token）
async function replaceVideoToken(video: VideoItem, url: string): Promise<string> {
  if (!video.website_name) return url

  try {
    // 根据视频的website_name获取对应网站的配置
    const website = await fetchWebsiteByName(video.website_name)
    if (website && website.local_storage) {
      const tokenItem = website.local_storage.find((item: { key: string }) => item.key === 'token')
      if (tokenItem) {
        const urlToken = extractUrlToken(url)
        if (urlToken !== tokenItem.value) {
          return replaceUrlToken(url, tokenItem.value)
        }
      }
    }
  } catch (e) {
    console.error('获取网站配置失败:', e)
  }
  return url
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
    const result = await searchVideosApi(
      searchQuery.value,
      currentPage.value,
      pageSize
    )

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
    const result = await scrapeVideoApi(
      selectedWebsite.value || '',
      videoId.value.trim()
    )

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

async function downloadVideoItem(video: VideoItem) {
  try {
    await downloadVideoApi(video.id)
    // 状态更新由 progress 事件和 videos-updated 事件处理
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
    await batchDownloadApi(ids)
    selectedIds.value.clear()
    // 不要在这里调用 loadVideos()，否则会清除进度状态
    // 状态更新由 progress 事件和 videos-updated 事件处理
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
        await deleteVideoApi(id)
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
          await deleteVideoApi(id)
        }
        selectedIds.value.clear()
        await loadVideos()
      } catch (e) {
        alert('批量删除失败: ' + e)
      }
    }
  }
}

async function clearDownloaded() {
  // 使用自定义确认对话框
  confirmDialog.value = {
    visible: true,
    message: '确定要清除所有已下载的视频吗？',
    onConfirm: async () => {
      try {
        await clearDownloadedVideos()
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
    [VideoStatus.Pending]: 'bg-amber-100 text-amber-700',
    [VideoStatus.Scraped]: 'bg-blue-100 text-blue-700',
    [VideoStatus.Downloading]: 'bg-green-100 text-green-700',
    [VideoStatus.Downloaded]: 'bg-green-100 text-green-700',
    [VideoStatus.Failed]: 'bg-red-100 text-red-700'
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
  let url = video.m3u8_url

  // 替换token - 根据视频的website_name获取对应网站的token
  replaceVideoToken(video, url).then(finalUrl => {
    playerSrc.value = finalUrl
    playerTitle.value = video.name
    playerVideoId.value = video.id
    // 设置播放列表和当前索引
    playerPlaylist.value = videos.value
    currentVideoIndex.value = videos.value.findIndex(v => v.id === video.id)
    playerVisible.value = true
  })
}


// 处理播放下一个视频
async function handlePlayNext(nextIndex: number) {
  if (nextIndex >= 0 && nextIndex < playerPlaylist.value.length) {
    const nextVideo = playerPlaylist.value[nextIndex]
    const finalUrl = await replaceVideoToken(nextVideo, nextVideo.m3u8_url)
    playerSrc.value = finalUrl
    playerTitle.value = nextVideo.name
    playerVideoId.value = nextVideo.id
    currentVideoIndex.value = nextIndex
  }
}

// 处理删除当前视频
async function handleDeleteCurrent() {
  const currentVideo = playerPlaylist.value[currentVideoIndex.value]
  if (!currentVideo) return

  try {
    // 调用删除接口
    await deleteVideoApi(currentVideo.id)
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

      // 替换token
      const finalUrl = await replaceVideoToken(nextVideo, nextVideo.m3u8_url)
      playerSrc.value = finalUrl
      playerTitle.value = nextVideo.name
      playerVideoId.value = nextVideo.id
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
  <div class="h-full flex flex-col bg-white rounded-xl shadow-[0_2px_12px_rgba(0,0,0,0.06)] overflow-hidden">
    <div class="flex gap-2.5 px-5 py-4 border-b border-[#f0f0f0] shrink-0">
      <select v-model="selectedWebsite" :disabled="isScraping" class="px-3.5 py-2.5 border border-[#e8e8e8] rounded-lg text-sm bg-white cursor-pointer min-w-[140px] transition-all focus:outline-none focus:border-[#667eea] focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)] disabled:opacity-60 disabled:cursor-not-allowed disabled:bg-[#fafbfc]">
        <option v-for="site in websites" :key="site.id" :value="site.id">{{ site.name }}</option>
      </select>
      <input type="text" v-model="videoId" placeholder="输入视频ID" @keyup.enter="scrape" :disabled="isScraping" class="flex-1 px-3.5 py-2.5 border border-[#e8e8e8] rounded-lg text-sm transition-all focus:outline-none focus:border-[#667eea] focus:shadow-[0_0_0_3px_rgba(102,126,234,0.1)]" />
      <button @click="scrape" :disabled="isScraping" class="px-6 py-2.5 text-white border-none rounded-lg text-sm font-medium cursor-pointer whitespace-nowrap transition-all bg-[linear-gradient(135deg,#667eea_0%,#764ba2_100%)] hover:-translate-y-0.5 hover:shadow-[0_4px_12px_rgba(102,126,234,0.35)] disabled:opacity-60 disabled:cursor-not-allowed">{{ isScraping ? '爬取中...' : '爬取' }}</button>
    </div>

    <div v-if="scrapeResult" :class="['mx-5 px-3.5 py-2.5 rounded-lg text-[13px] shrink-0', scrapeResult.success ? 'bg-[#f0fdf4] text-[#166534] border border-[#bbf7d0]' : 'bg-[#fef2f2] text-[#991b1b] border border-[#fecaca]']">
      <span v-if="scrapeResult.success">{{ scrapeResult.name }}</span>
      <span v-else>{{ scrapeResult.message }}</span>
    </div>

    <div class="flex-1 flex flex-col overflow-hidden">
      <div class="flex justify-between items-center px-5 py-3 bg-[#fafbfc] border-b border-[#f0f0f0] shrink-0">
        <div class="flex items-center gap-3">
          <span class="text-sm font-semibold text-[#1a1a2e]">视频列表 ({{ filteredVideos.length }}/{{ videos.length }})</span>
          <span v-if="copyMessage" class="text-[13px] text-[#22c55e] font-medium">{{ copyMessage }}</span>
        </div>
        <div class="flex items-center gap-2.5">
          <input type="text" v-model="searchQuery" @input="filterVideos" placeholder="搜索视频名称" class="px-3 py-1.5 border border-[#e8e8e8] rounded-md text-[13px] w-[180px] transition-all focus:outline-none focus:border-[#667eea]" />
          <select v-model="statusFilter" @change="filterVideos" class="px-3 py-1.5 border border-[#e8e8e8] rounded-md text-[13px] bg-white cursor-pointer transition-all focus:outline-none focus:border-[#667eea]">
            <option value="">全部状态</option>
            <option :value="VideoStatus.Pending">待爬取</option>
            <option :value="VideoStatus.Scraped">已爬取</option>
            <option :value="VideoStatus.Downloading">下载中</option>
            <option :value="VideoStatus.Downloaded">已下载</option>
            <option :value="VideoStatus.Failed">失败</option>
          </select>
          <button v-if="selectedIds.size > 0" @click="batchDownload" class="px-3.5 py-1.5 border-none rounded-md text-xs font-medium cursor-pointer transition-all bg-[#22c55e] text-white hover:bg-[#16a34a]">下载选中 ({{ selectedIds.size }})</button>
          <button v-if="selectedIds.size > 0" @click="deleteSelected" class="px-3.5 py-1.5 border-none rounded-md text-xs font-medium cursor-pointer transition-all bg-[#fee2e2] text-[#dc2626] hover:bg-[#fecaca]">删除选中</button>
          <button v-if="videos.some(v => v.status === VideoStatus.Downloaded)" @click="clearDownloaded" class="px-3 py-1 bg-transparent text-[#667eea] border border-[#667eea] rounded-md text-xs cursor-pointer transition-all hover:bg-[#667eea] hover:text-white">清除已下载</button>
        </div>
      </div>

      <div class="flex-1 flex flex-col overflow-hidden">
        <div class="flex px-5 py-2.5 bg-[#f8f9fa] border-b border-[#eee] text-xs font-semibold text-[#64748b] uppercase tracking-[0.5px] items-center">
          <div class="w-8 shrink-0">
            <input type="checkbox" @change="toggleSelectAll" :checked="filteredVideos.length > 0 && filteredVideos.every(v => selectedIds.has(v.id) || v.status === VideoStatus.Downloaded || v.status === VideoStatus.Downloading)" :indeterminate="selectedIds.size > 0 && !filteredVideos.every(v => selectedIds.has(v.id) || v.status === VideoStatus.Downloaded || v.status === VideoStatus.Downloading)" class="w-4 h-4 cursor-pointer" />
          </div>
          <span class="flex-1 min-w-0 pr-4">名称</span>
          <span class="w-[100px] pr-4">状态</span>
          <span class="w-[150px] shrink-0">操作</span>
        </div>

        <div class="flex-1 overflow-y-auto" @scroll="handleScroll" ref="tableBodyRef">
          <div v-if="videos.length === 0 && !isLoadingMore" class="py-10 px-5 text-center text-[#94a3b8] text-[13px]">输入视频ID开始爬取</div>
          <div v-else-if="filteredVideos.length === 0 && !isLoadingMore" class="py-10 px-5 text-center text-[#94a3b8] text-[13px]">没有找到匹配的视频</div>
          <div v-else-if="isLoadingMore && videos.length === 0" class="py-10 px-5 text-center text-[#94a3b8] text-[13px]">加载中...</div>

          <div v-for="video in filteredVideos" :key="video.id" class="flex items-center px-5 py-3 border-b border-[#f5f5f5] transition-colors hover:bg-[#fafbfc]">
            <div class="w-8 shrink-0">
              <input type="checkbox" :checked="selectedIds.has(video.id)" :disabled="video.status === VideoStatus.Downloaded || video.status === VideoStatus.Downloading" @change="toggleSelect(video.id)" class="w-4 h-4 cursor-pointer" />
            </div>
            <div class="w-[60px] h-[34px] mr-3 cursor-pointer relative overflow-hidden rounded bg-[#f5f5f5] shrink-0" @click="openPlayer(video)">
              <img v-if="video.cover_url" :src="video.cover_url" :alt="video.name" class="w-full h-full object-cover transition-transform duration-200 hover:scale-[1.2]" @error="handleImageError" @mouseenter="showCoverPopup($event, video.cover_url)" @mousemove="moveCoverPopup($event)" @mouseleave="hideCoverPopup" />
              <div v-else class="w-full h-full flex items-center justify-center bg-[#f8f9fa] text-[#cbd5e1]">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"></rect>
                  <line x1="7" y1="2" x2="7" y2="22"></line>
                  <line x1="17" y1="2" x2="17" y2="22"></line>
                  <line x1="2" y1="12" x2="22" y2="12"></line>
                </svg>
              </div>
            </div>
            <div class="flex-1 min-w-0 pr-4">
              <span class="block text-sm font-medium text-[#1a1a2e] whitespace-nowrap overflow-hidden text-ellipsis" :title="video.name">{{ video.name }}</span>
              <div class="flex gap-2 mt-1">
                <span v-if="video.view_count !== undefined && video.view_count !== null" class="inline-flex items-center gap-[3px] px-1.5 py-0.5 rounded text-[11px] font-medium bg-[#f0f9ff] text-[#0369a1]">
                  <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polygon points="5 3 19 12 5 21 5 3"></polygon>
                  </svg>
                  {{ formatCount(video.view_count) }}
                </span>
                <span v-if="video.favorite_count !== undefined && video.favorite_count !== null" class="inline-flex items-center gap-[3px] px-1.5 py-0.5 rounded text-[11px] font-medium bg-[#fef2f2] text-[#dc2626]">
                  <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"></path>
                  </svg>
                  {{ formatCount(video.favorite_count) }}
                </span>
              </div>
            </div>
            <div class="w-[100px] pr-4">
              <div v-if="isDownloading(video)" class="w-full">
                <div class="h-1.5 bg-[#e5e7eb] rounded-[3px] overflow-hidden mb-1">
                  <div class="h-full rounded-[3px] transition-all duration-300 bg-[linear-gradient(90deg,#22c55e,#16a34a)]" :style="{ width: getProgress(downloadProgress[video.id]) + '%' }"></div>
                </div>
                <div class="flex justify-between items-center text-[10px]">
                  <span class="text-[#16a34a] font-semibold">{{ Math.round(getProgress(downloadProgress[video.id])) }}%</span>
                  <span class="text-[#64748b]">{{ downloadProgress[video.id]?.speed || '0 MB/s' }}</span>
                </div>
              </div>
              <span v-else :class="['inline-block px-2.5 py-1 rounded-full text-[11px] font-medium', getStatusClass(video.status)]">{{ getStatusText(video.status) }}</span>
            </div>
            <div class="w-[150px] flex items-center gap-1.5 shrink-0">
              <IconButton variant="play" title="播放" @click="openPlayer(video)" />
              <IconButton variant="cast" title="DLNA 投屏" @click="openDlnaDialog(video)" />
              <IconButton variant="download" title="下载" @click="downloadVideoItem(video)" />
              <IconButton variant="delete" title="删除" @click="deleteVideo(video.id)" />
            </div>
          </div>

          <div v-if="hasMore || isLoadingMore" class="p-4 text-center">
            <button v-if="hasMore && !isLoadingMore" @click="loadMore" class="px-6 py-2 bg-[#f1f5f9] text-[#64748b] border-none rounded-md text-[13px] cursor-pointer transition-all hover:bg-[#e2e8f0] hover:text-[#475569]">加载更多</button>
            <span v-else-if="isLoadingMore" class="text-[13px] text-[#94a3b8]">加载中...</span>
          </div>

          <div v-if="videos.length > 0" class="py-3 px-5 text-right text-xs text-[#94a3b8] border-t border-[#f0f0f0]">共 {{ total }} 条视频，当前显示 {{ videos.length }} 条</div>
        </div>
      </div>
    </div>

    <LogPopup ref="logPopupRef" :visible="logPopupVisible" title="爬取日志" @close="handleLogPopupClose" />

    <Teleport to="body">
      <div v-if="confirmDialog.visible" class="fixed inset-0 bg-black/50 flex items-center justify-center z-[1000]" @click="handleCancel">
        <div class="bg-white rounded-xl p-6 min-w-[300px] shadow-[0_4px_20px_rgba(0,0,0,0.15)]" @click.stop>
          <div class="text-[15px] text-[#1a1a2e] mb-5 text-center">{{ confirmDialog.message }}</div>
          <div class="flex gap-3 justify-center">
            <button class="px-6 py-2 border-none rounded-lg text-sm font-medium cursor-pointer transition-all bg-[#f1f5f9] text-[#64748b] hover:bg-[#e2e8f0]" @click="handleCancel">取消</button>
            <button class="px-6 py-2 border-none rounded-lg text-sm font-medium cursor-pointer transition-all bg-[#4f46e5] text-white hover:bg-[#4338ca]" @click="handleConfirm">确定</button>
          </div>
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="coverPopupVisible" class="fixed z-[1000] p-2 bg-white rounded-lg shadow-[0_4px_20px_rgba(0,0,0,0.2)] pointer-events-auto" :style="{ left: coverPopupPosition.x + 20 + 'px', top: coverPopupPosition.y - 150 + 'px' }" @mouseleave="hideCoverPopup">
        <img :src="coverPopupImage" alt="封面预览" class="w-[400px] h-auto object-contain rounded block" />
      </div>
    </Teleport>

    <VideoPlayer :visible="playerVisible" :src="playerSrc" :title="playerTitle" :playlist="playerPlaylist" :current-index="currentVideoIndex" :video-id="playerVideoId" @close="handlePlayerClose" @play-next="handlePlayNext" @delete-current="handleDeleteCurrent" />

    <DlnaCastDialog v-if="dlnaVideo" :video="dlnaVideo" @close="closeDlnaDialog" />
  </div>
</template>

<style scoped>
</style>
