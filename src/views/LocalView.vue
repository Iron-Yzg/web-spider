<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import type { LocalVideo } from '../types'
import VideoPlayer from '../components/VideoPlayer.vue'

const videos = ref<LocalVideo[]>([])
const searchQuery = ref('')
const filteredVideos = ref<LocalVideo[]>([])
const isLoading = ref(false)
const selectDialog = ref<{ visible: boolean, message: string, onConfirm: (() => void) | null }>({
  visible: false,
  message: '',
  onConfirm: null
})

// 播放器状态
const playerVisible = ref(false)
const playerSrc = ref('')
const playerTitle = ref('')
const playerPlaylist = ref<LocalVideo[]>([])
const currentVideoIndex = ref(0)

// 获取应用数据目录
async function getAppDataDir(): Promise<string> {
  return await invoke<string>('get_app_data_dir')
}

// 加载视频列表
async function loadVideos() {
  try {
    const dataDir = await getAppDataDir()
    const jsonPath = `${dataDir}/local_videos.json`

    try {
      const data = await invoke<LocalVideo[]>('read_local_videos', { path: jsonPath })
      videos.value = data || []
    } catch {
      // 文件不存在，创建空列表
      videos.value = []
    }

    filterVideos()
  } catch (e) {
    console.error('加载视频列表失败:', e)
  }
}

// 保存视频列表
async function saveVideos() {
  try {
    const dataDir = await getAppDataDir()
    const jsonPath = `${dataDir}/local_videos.json`

    await invoke('write_local_videos', { path: jsonPath, data: videos.value })
  } catch (e) {
    console.error('保存视频列表失败:', e)
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

// 选择文件夹并添加视频
async function selectVideos() {
  try {
    // 使用 Tauri dialog 选择文件
    const result: string[] | null = await invoke<string[] | null>('select_video_files')

    if (!result || result.length === 0) return

    isLoading.value = true

    for (const filePath of result) {
      const info = await getVideoInfo(filePath)
      if (info) {
        // 检查是否已存在
        const exists = videos.value.some(v => v.file_path === filePath)
        if (!exists) {
          videos.value.push(info)
        }
      }
    }

    await saveVideos()
    filterVideos()
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
  // 使用 convertFileSrc 转换本地路径为 asset URL
  playerSrc.value = convertFileSrc(video.file_path)
  playerTitle.value = video.name
  playerPlaylist.value = filteredVideos.value
  currentVideoIndex.value = filteredVideos.value.findIndex(v => v.id === video.id)
  playerVisible.value = true
}

// 处理播放下一个视频
function handlePlayNext(nextIndex: number) {
  if (nextIndex >= 0 && nextIndex < playerPlaylist.value.length) {
    const nextVideo = playerPlaylist.value[nextIndex]
    // 使用 convertFileSrc 转换本地路径为 asset URL
    playerSrc.value = convertFileSrc(nextVideo.file_path)
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
      const index = videos.value.findIndex(v => v.id === id)
      if (index > -1) {
        videos.value.splice(index, 1)
        await saveVideos()
        filterVideos()
      }
    }
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
          <span class="col-name">名称</span>
          <span class="col-size">大小</span>
          <span class="col-duration">时长</span>
          <span class="col-resolution">分辨率</span>
          <span class="col-added">添加时间</span>
          <span class="col-action">操作</span>
        </div>

        <div class="table-body">
          <div v-for="video in filteredVideos" :key="video.id" class="table-row">
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
              <button @click="deleteVideo(video.id)" class="action-btn delete" title="删除">
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18"></line>
                  <line x1="6" y1="6" x2="18" y2="18"></line>
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
      @delete-current="() => {}"
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
  width: 80px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-duration {
  width: 80px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-resolution {
  width: 100px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-added {
  width: 100px;
  font-size: 13px;
  color: #64748b;
  flex-shrink: 0;
}

.col-action {
  width: 80px;
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
