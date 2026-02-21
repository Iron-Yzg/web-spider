<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import type { LocalVideo } from '../types'
import VideoPlayer from '../components/VideoPlayer.vue'
import DlnaCastDialog from '../components/DlnaCastDialog.vue'
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

function openDlnaDialog(video: LocalVideo) {
  dlnaVideo.value = video
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
  <div class="local-page">
    <!-- 顶部工具栏 -->
    <div class="toolbar">
      <button @click="selectVideos" :disabled="isLoading" class="add-btn">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
          <polyline points="17 8 12 3 7 8"></polyline>
          <line x1="12" y1="3" x2="12" y2="15"></line>
        </svg>
        {{ isLoading ? '加载中...' : '添加视频' }}
      </button>

      <div class="search-box">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <input
          type="text"
          v-model="searchQuery"
          @input="handleSearch"
          placeholder="搜索视频名称"
        />
      </div>

      <span class="video-count">共 {{ filteredVideos.length }} 个视频</span>
    </div>

    <!-- 视频列表 -->
    <div class="video-section">
      <div v-if="filteredVideos.length === 0 && !isLoading" class="empty-tip">
        <div class="empty-icon">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"></rect>
            <line x1="7" y1="2" x2="7" y2="22"></line>
            <line x1="17" y1="2" x2="17" y2="22"></line>
            <line x1="2" y1="12" x2="22" y2="12"></line>
            <line x1="12" y1="2" x2="12" y2="22"></line>
          </svg>
        </div>
        <p>暂无本地视频</p>
        <p class="empty-hint">点击"添加视频"按钮选择本地视频文件</p>
      </div>

      <div v-else class="video-table">
        <div class="table-header">
          <span class="col-thumbnail">封面</span>
          <span class="col-name">名称</span>
          <span class="col-size">大小</span>
          <span class="col-duration">时长</span>
          <span class="col-resolution">分辨率</span>
          <span class="col-added">添加时间</span>
          <span class="col-action">操作</span>
        </div>

        <div class="table-body">
          <div v-for="video in filteredVideos" :key="video.id" class="table-row">
            <div class="col-thumbnail">
              <div class="video-thumbnail" @click="playVideo(video)">
                <svg class="play-icon" viewBox="0 0 24 24" fill="currentColor">
                  <polygon points="5 3 19 12 5 21 5 3"></polygon>
                </svg>
              </div>
            </div>
            <div class="col-name">
              <span class="video-name" :title="video.name">{{ video.name }}</span>
              <span class="video-path" :title="video.file_path">{{ video.file_path }}</span>
            </div>
            <div class="col-size">{{ video.file_size }}</div>
            <div class="col-duration">{{ video.duration }}</div>
            <div class="col-resolution">{{ video.resolution }}</div>
            <div class="col-added">{{ video.added_at.split('T')[0] }}</div>
            <div class="col-action">
              <button @click="playVideo(video)" class="action-btn play" title="播放">
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="none">
                  <polygon points="5 3 19 12 5 21 5 3"></polygon>
                </svg>
              </button>
              <button @click="openDlnaDialog(video)" class="action-btn cast" title="投屏">
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="2" y="7" width="20" height="15" rx="2" ry="2"></rect>
                  <polyline points="17 2 12 7 7 2"></polyline>
                </svg>
              </button>
              <button @click="deleteVideo(video.id)" class="action-btn delete" title="删除">
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="3 6 5 6 21 6"></polyline>
                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
                </svg>
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 确认对话框 -->
    <Teleport to="body">
      <div v-if="selectDialog.visible" class="confirm-overlay" @click="handleCancel">
        <div class="confirm-dialog" @click.stop>
          <div class="confirm-content">{{ selectDialog.message }}</div>
          <div class="confirm-actions">
            <button class="confirm-btn cancel" @click="handleCancel">取消</button>
            <button class="confirm-btn ok" @click="handleConfirm">确定</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- 视频播放器 -->
    <VideoPlayer
      :visible="playerVisible"
      :src="playerSrc"
      :title="playerTitle"
      :playlist="playerPlaylist"
      :current-index="currentVideoIndex"
      video-id=""
      @close="handlePlayerClose"
      @play-next="handlePlayNext"
      @delete-current="handleDeleteCurrent"
    />

    <!-- DLNA 投屏弹窗 -->
    <DlnaCastDialog
      v-if="dlnaVideo"
      :video="dlnaVideo"
      @close="closeDlnaDialog"
    />
  </div>
</template>

<style scoped>
.local-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: #fff;
  border-radius: 12px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.06);
  overflow: hidden;
}

/* 工具栏 */
.toolbar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 20px;
  border-bottom: 1px solid #f0f0f0;
  flex-shrink: 0;
}

.add-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 20px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.add-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.add-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.search-box {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  background: #f5f6f8;
  border-radius: 8px;
  flex: 1;
  max-width: 400px;
}

.search-box svg {
  color: #94a3b8;
  flex-shrink: 0;
}

.search-box input {
  flex: 1;
  border: none;
  background: transparent;
  font-size: 14px;
  outline: none;
  color: #1a1a2e;
}

.search-box input::placeholder {
  color: #94a3b8;
}

.video-count {
  font-size: 13px;
  color: #64748b;
  white-space: nowrap;
}

/* 视频区域 */
.video-section {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.empty-tip {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: #94a3b8;
}

.empty-icon {
  margin-bottom: 16px;
  color: #cbd5e1;
}

.empty-tip p {
  margin: 0;
  font-size: 14px;
}

.empty-hint {
  margin-top: 8px !important;
  font-size: 12px !important;
  color: #cbd5e1;
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

.col-thumbnail {
  width: 60px;
  height: 34px;
  flex-shrink: 0;
  margin-right: 12px;
  position: relative;
  overflow: hidden;
  border-radius: 4px;
  background: #f5f5f5;
}

.video-thumbnail {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.2s;
}

.video-thumbnail:hover {
  opacity: 0.9;
  background: linear-gradient(135deg, #16213e 0%, #1a1a2e 100%);
}

.video-thumbnail .play-icon {
  width: 20px;
  height: 20px;
  color: rgba(255, 255, 255, 0.8);
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

.video-path {
  display: block;
  font-size: 11px;
  color: #94a3b8;
  font-family: monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.col-size {
  width: 60px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-duration {
  width: 60px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-resolution {
  width: 80px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-added {
  width: 80px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-action {
  width: 110px;
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.action-btn {
  padding: 5px 6px;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.action-btn.play {
  background: #fef3c7;
  color: #d97706;
}

.action-btn.play:hover {
  background: #fde68a;
}

.action-btn.cast {
  background: #e0e7ff;
  color: #4f46e5;
}

.action-btn.cast:hover {
  background: #c7d2fe;
}

.action-btn.delete {
  background: #fee2e2;
  color: #dc2626;
}

.action-btn.delete:hover {
  background: #fecaca;
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
</style>
