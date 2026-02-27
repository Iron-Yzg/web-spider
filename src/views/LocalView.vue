<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { LocalVideo } from '../types'
import VideoPlayer from '../components/VideoPlayer.vue'
import DlnaCastDialog from '../components/DlnaCastDialog.vue'
import IconButton from '../components/IconButton.vue'
import { getLocalVideos, addLocalVideo, deleteLocalVideo as deleteLocalVideoApi } from '../services/api'

const videos = ref<LocalVideo[]>([])
const searchQuery = ref('')
const filteredVideos = ref<LocalVideo[]>([])
const isLoading = ref(false)
const selectDialog = ref<{ visible: boolean, message: string, onConfirm: (() => void) | null }>({
  visible: false,
  message: '',
  onConfirm: null
})

// DLNA 投屏弹窗
const showDlnaDialog = ref(false)
const dlnaVideo = ref<LocalVideo | null>(null)
const dlnaPlaylist = ref<LocalVideo[]>([])
const dlnaCurrentIndex = ref(0)

function openDlnaDialog(video: LocalVideo) {
  dlnaVideo.value = video
  dlnaPlaylist.value = filteredVideos.value
  const idx = filteredVideos.value.findIndex(v => v.id === video.id)
  dlnaCurrentIndex.value = idx >= 0 ? idx : 0
  showDlnaDialog.value = true
}

function closeDlnaDialog() {
  showDlnaDialog.value = false
  dlnaVideo.value = null
}

// 播放器状态
const playerVisible = ref(false)
const playerSrc = ref('')
const playerTitle = ref('')
const playerPlaylist = ref<LocalVideo[]>([])
const currentVideoIndex = ref(0)

// 加载视频列表（使用数据库）
async function loadVideos() {
  try {
    isLoading.value = true
    const data = await getLocalVideos()
    videos.value = data || []
    filterVideos()
  } catch (e) {
    console.error('加载视频列表失败:', e)
    videos.value = []
  } finally {
    isLoading.value = false
  }
}

// 筛选视频
function filterVideos() {
  if (!searchQuery.value.trim()) {
    filteredVideos.value = videos.value
  } else {
    const query = searchQuery.value.toLowerCase()
    filteredVideos.value = videos.value.filter(v =>
      v.name.toLowerCase().includes(query) ||
      v.file_path.toLowerCase().includes(query)
    )
  }
}

// 带超时的 promise 包装
function withTimeout<T>(promise: Promise<T>, ms: number, errorMsg: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => 
      setTimeout(() => reject(new Error(errorMsg)), ms)
    )
  ])
}

// 限制并发的异步处理器
async function processWithConcurrency<T, R>(
  items: T[],
  processor: (item: T) => Promise<R>,
  concurrency: number
): Promise<(R | null)[]> {
  const results: (R | null)[] = new Array(items.length).fill(null)
  
  async function processBatch(startIdx: number) {
    const batch = items.slice(startIdx, startIdx + concurrency)
    const batchPromises = batch.map((item, idx) => 
      processor(item).then(result => {
        results[startIdx + idx] = result
      }).catch(e => {
        console.error(`处理第 ${startIdx + idx} 项失败:`, e)
        results[startIdx + idx] = null
      })
    )
    await Promise.all(batchPromises)
  }
  
  for (let i = 0; i < items.length; i += concurrency) {
    await processBatch(i)
  }
  
  return results
}

// 选择文件夹并添加视频
async function selectVideos() {
  try {
    // 使用 Tauri dialog 选择文件
    const result: string[] | null = await invoke<string[] | null>('select_video_files')

    if (!result || result.length === 0) return

    isLoading.value = true
    console.log(`[LocalView] 开始处理 ${result.length} 个视频文件`)

    // 使用限制并发(3个)的并行处理，避免同时扫描多个大文件导致卡死
    const infos = await processWithConcurrency(
      result,
      async (filePath) => {
        // 每个文件处理添加 10 秒超时
        return withTimeout(
          getVideoInfo(filePath),
          10000,
          '获取视频信息超时'
        ).catch(e => {
          console.warn(`[LocalView] 获取视频信息失败或超时: ${filePath}`, e)
          return null
        })
      },
      3 // 最多同时处理 3 个文件
    )

    // 保存到数据库
    for (const info of infos) {
      if (info) {
        // 检查是否已存在
        const exists = videos.value.some(v => v.file_path === info.file_path)
        if (!exists) {
          // 添加到数据库
          await addLocalVideo(info)
          videos.value.push(info)
        }
      }
    }

    filterVideos()
    console.log(`[LocalView] 成功添加视频，当前共 ${videos.value.length} 个`)
  } catch (e) {
    console.error('选择视频失败:', e)
  } finally {
    isLoading.value = false
  }
}

// 获取视频信息（使用 ffprobe 获取文件信息）
async function getVideoInfo(filePath: string): Promise<LocalVideo | null> {
  try {
    // 获取文件名作为标题
    const name = filePath.split('/').pop()?.split('\\').pop()?.replace(/\.[^/.]+$/, '') || '未知视频'

    // 使用 ffprobe 获取视频信息
    const [resolution, duration, fileSize] = await invoke<[string, string, string]>('get_media_info', { path: filePath })

    return {
      id: crypto.randomUUID(),
      name,
      file_path: filePath,
      file_size: fileSize,
      duration,
      resolution,
      added_at: new Date().toISOString()
    }
  } catch (e) {
    console.error('获取视频信息失败:', e)
    // 如果获取失败，使用文件大小作为备选
    try {
      const stats = await invoke<{ size: number }>('get_file_stats', { path: filePath })
      const fileSize = formatFileSize(stats.size)
      return {
        id: crypto.randomUUID(),
        name: filePath.split('/').pop()?.split('\\').pop()?.replace(/\.[^/.]+$/, '') || '未知视频',
        file_path: filePath,
        file_size: fileSize,
        duration: '未知',
        resolution: '未知',
        added_at: new Date().toISOString()
      }
    } catch {
      return null
    }
  }
}

// 格式化文件大小
function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

// 播放视频
function playVideo(video: LocalVideo) {
  // 传递原始文件路径，由 VideoPlayer 内部处理路径转换
  playerSrc.value = video.file_path
  playerTitle.value = video.name
  playerPlaylist.value = filteredVideos.value
  currentVideoIndex.value = filteredVideos.value.findIndex(v => v.id === video.id)
  playerVisible.value = true
}

// 处理播放下一个视频
function handlePlayNext(nextIndex: number) {
  if (nextIndex >= 0 && nextIndex < playerPlaylist.value.length) {
    const nextVideo = playerPlaylist.value[nextIndex]
    // 传递原始文件路径，由 VideoPlayer 内部处理路径转换
    playerSrc.value = nextVideo.file_path
    playerTitle.value = nextVideo.name
    currentVideoIndex.value = nextIndex
  }
}

// 删除视频
function deleteVideo(id: string) {
  selectDialog.value = {
    visible: true,
    message: '确定要删除这个视频吗？',
    onConfirm: async () => {
      await deleteLocalVideoApi(id)
      await loadVideos()
    }
  }
}

// 处理删除当前视频（从播放器中删除）
async function handleDeleteCurrent() {
  const currentVideo = playerPlaylist.value[currentVideoIndex.value]
  if (!currentVideo) return

  // 直接删除，不询问
  await deleteLocalVideoApi(currentVideo.id)
  await loadVideos()

  // 从播放列表中移除
  playerPlaylist.value.splice(currentVideoIndex.value, 1)

  // 如果还有视频，播放下一个
  if (playerPlaylist.value.length > 0) {
    const nextIndex = currentVideoIndex.value >= playerPlaylist.value.length
      ? playerPlaylist.value.length - 1
      : currentVideoIndex.value
    const nextVideo = playerPlaylist.value[nextIndex]
    // 传递原始文件路径，由 VideoPlayer 内部处理路径转换
    playerSrc.value = nextVideo.file_path
    playerTitle.value = nextVideo.name
    currentVideoIndex.value = nextIndex
  } else {
    // 没有视频了，关闭播放器
    handlePlayerClose()
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

// 处理确认对话框
function handleConfirm() {
  if (selectDialog.value.onConfirm) {
    selectDialog.value.onConfirm()
  }
  selectDialog.value.visible = false
  selectDialog.value.onConfirm = null
}

// 处理取消
function handleCancel() {
  selectDialog.value.visible = false
  selectDialog.value.onConfirm = null
}

// 搜索处理
function handleSearch() {
  filterVideos()
}

onMounted(async () => {
  await loadVideos()
})
</script>

<template>
  <div class="h-full flex flex-col bg-white rounded-xl shadow-[0_2px_12px_rgba(0,0,0,0.06)] overflow-hidden">
    <div class="flex items-center gap-4 px-5 py-4 border-b border-[#f0f0f0] shrink-0">
      <button @click="selectVideos" :disabled="isLoading" class="flex items-center gap-2 px-5 py-2.5 border-none rounded-lg text-sm font-medium text-white cursor-pointer transition-all bg-[linear-gradient(135deg,#667eea_0%,#764ba2_100%)] hover:-translate-y-0.5 hover:shadow-[0_4px_12px_rgba(102,126,234,0.35)] disabled:opacity-60 disabled:cursor-not-allowed">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
          <polyline points="17 8 12 3 7 8"></polyline>
          <line x1="12" y1="3" x2="12" y2="15"></line>
        </svg>
        {{ isLoading ? '加载中...' : '添加视频' }}
      </button>

      <div class="flex items-center gap-2 px-3.5 py-2 bg-[#f5f6f8] rounded-lg flex-1 max-w-[400px]">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-[#94a3b8] shrink-0">
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <input type="text" v-model="searchQuery" @input="handleSearch" placeholder="搜索视频名称" class="flex-1 border-none bg-transparent text-sm outline-none text-[#1a1a2e] placeholder:text-[#94a3b8]" />
      </div>

      <span class="text-[13px] text-[#64748b] whitespace-nowrap">共 {{ filteredVideos.length }} 个视频</span>
    </div>

    <div class="flex-1 flex flex-col overflow-hidden">
      <div v-if="filteredVideos.length === 0 && !isLoading" class="flex-1 flex flex-col items-center justify-center p-10 text-[#94a3b8]">
        <div class="mb-4 text-[#cbd5e1]">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"></rect>
            <line x1="7" y1="2" x2="7" y2="22"></line>
            <line x1="17" y1="2" x2="17" y2="22"></line>
            <line x1="2" y1="12" x2="22" y2="12"></line>
            <line x1="12" y1="2" x2="12" y2="22"></line>
          </svg>
        </div>
        <p class="text-sm">暂无本地视频</p>
        <p class="mt-2 text-xs text-[#cbd5e1]">点击"添加视频"按钮选择本地视频文件</p>
      </div>

      <div v-else class="flex-1 flex flex-col overflow-hidden">
        <div class="flex px-5 py-2.5 bg-[#f8f9fa] border-b border-[#eee] text-xs font-semibold text-[#64748b] uppercase tracking-[0.5px] items-center">
          <span class="w-[60px] h-[34px] shrink-0 mr-3">封面</span>
          <span class="flex-1 min-w-0 pr-4">名称</span>
          <span class="w-[60px] text-[13px] normal-case">大小</span>
          <span class="w-[60px] text-[13px] normal-case">时长</span>
          <span class="w-[80px] text-[13px] normal-case">分辨率</span>
          <span class="w-[80px] text-[13px] normal-case">添加时间</span>
          <span class="w-[110px] text-[13px] normal-case">操作</span>
        </div>

        <div class="flex-1 overflow-y-auto">
          <div v-for="video in filteredVideos" :key="video.id" class="flex items-center px-5 py-3 border-b border-[#f5f5f5] transition-colors hover:bg-[#fafbfc]">
            <div class="w-[60px] h-[34px] mr-3 relative overflow-hidden rounded bg-[#f5f5f5] shrink-0">
              <div class="w-full h-full flex items-center justify-center rounded bg-[linear-gradient(135deg,#1a1a2e_0%,#16213e_100%)] cursor-pointer transition-all hover:opacity-90" @click="playVideo(video)">
                <svg class="w-5 h-5 text-white/80" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
              </div>
            </div>
            <div class="flex-1 min-w-0 pr-4">
              <span class="block text-sm font-medium text-[#1a1a2e] whitespace-nowrap overflow-hidden text-ellipsis" :title="video.name">{{ video.name }}</span>
              <span class="block mt-0.5 text-[11px] text-[#94a3b8] font-mono whitespace-nowrap overflow-hidden text-ellipsis" :title="video.file_path">{{ video.file_path }}</span>
            </div>
            <div class="w-[60px] text-[13px] text-[#64748b] shrink-0">{{ video.file_size }}</div>
            <div class="w-[60px] text-[13px] text-[#64748b] shrink-0">{{ video.duration }}</div>
            <div class="w-[80px] text-[13px] text-[#64748b] shrink-0">{{ video.resolution }}</div>
            <div class="w-[80px] text-[13px] text-[#64748b] shrink-0">{{ video.added_at.split('T')[0] }}</div>
            <div class="w-[110px] flex items-center gap-1 shrink-0">
              <IconButton variant="play" title="播放" @click="playVideo(video)" />
              <IconButton variant="cast" title="投屏" @click="openDlnaDialog(video)" />
              <IconButton variant="delete" title="删除" @click="deleteVideo(video.id)" />
            </div>
          </div>
        </div>
      </div>
    </div>

    <Teleport to="body">
      <div v-if="selectDialog.visible" class="fixed inset-0 bg-black/50 flex items-center justify-center z-[1000]" @click="handleCancel">
        <div class="bg-white rounded-xl p-6 min-w-[300px] shadow-[0_4px_20px_rgba(0,0,0,0.15)]" @click.stop>
          <div class="text-[15px] text-[#1a1a2e] mb-5 text-center">{{ selectDialog.message }}</div>
          <div class="flex gap-3 justify-center">
            <button class="px-6 py-2 border-none rounded-lg text-sm font-medium cursor-pointer transition-all bg-[#f1f5f9] text-[#64748b] hover:bg-[#e2e8f0]" @click="handleCancel">取消</button>
            <button class="px-6 py-2 border-none rounded-lg text-sm font-medium cursor-pointer transition-all bg-[#4f46e5] text-white hover:bg-[#4338ca]" @click="handleConfirm">确定</button>
          </div>
        </div>
      </div>
    </Teleport>

    <VideoPlayer :visible="playerVisible" :src="playerSrc" :title="playerTitle" :playlist="playerPlaylist" :current-index="currentVideoIndex" video-id="" @close="handlePlayerClose" @play-next="handlePlayNext" @delete-current="handleDeleteCurrent" />

    <DlnaCastDialog v-if="dlnaVideo" :video="dlnaVideo" :playlist="dlnaPlaylist" :current-index="dlnaCurrentIndex" @close="closeDlnaDialog" />
  </div>
</template>

<style scoped>
</style>
