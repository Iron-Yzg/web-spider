<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { getVideoInfo, addYtdlpTasks, getYtdlpConfig } from '../services/api'
import type { YtdlpTask, YtdlpTaskStatus } from '../types'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'confirm', tasks: YtdlpTask[]): void
}>()

// 视频信息列表
interface VideoInfo {
  url: string
  title: string
  resolution: string
  fileSize: string
  loading: boolean
  error?: string
}

const urlInput = ref('')
const videoInfos = ref<VideoInfo[]>([])
const errorMessage = ref('')
const isAdding = ref(false)
const ytdlpQuality = ref(1080)

// 获取yt-dlp配置中的质量设置
async function loadYtdlpConfig() {
  try {
    const config = await getYtdlpConfig()
    ytdlpQuality.value = config.quality || 1080
  } catch (e) {
    console.error('获取yt-dlp配置失败:', e)
  }
}

// 重置状态
watch(() => props.visible, async (val) => {
  if (val) {
    urlInput.value = ''
    videoInfos.value = []
    errorMessage.value = ''
    isAdding.value = false
    await loadYtdlpConfig()
  }
})

// 验证 URL
function isValidUrl(url: string): boolean {
  try {
    new URL(url)
    return true
  } catch {
    return false
  }
}

// 添加 URL 并获取视频信息
async function addUrl() {
  const url = urlInput.value.trim()
  if (!url) {
    errorMessage.value = '请输入视频链接'
    return
  }
  if (!isValidUrl(url)) {
    errorMessage.value = '链接格式不正确'
    return
  }

  // 检查是否已存在
  const existingIndex = videoInfos.value.findIndex(v => v.url === url)
  if (existingIndex !== -1) {
    errorMessage.value = '该链接已添加'
    return
  }

  // 添加到列表并获取信息
  const newInfo: VideoInfo = {
    url,
    title: '加载中...',
    resolution: '-',
    fileSize: '-',
    loading: true,
  }
  videoInfos.value.push(newInfo)
  errorMessage.value = ''
  urlInput.value = ''

  // 获取视频信息
  try {
    const info = await getVideoInfo(url, ytdlpQuality.value)
    const index = videoInfos.value.findIndex(v => v.url === url)
    if (index !== -1) {
      videoInfos.value[index] = {
        url,
        title: info.title || '未知标题',
        resolution: info.resolution || '-',
        fileSize: info.file_size || '-',
        loading: false,
      }
    }
  } catch (e: any) {
    const index = videoInfos.value.findIndex(v => v.url === url)
    if (index !== -1) {
      videoInfos.value[index].loading = false
      videoInfos.value[index].error = e.message || '获取失败'
    }
  }
}

// 解析粘贴的多个 URL
async function handlePaste(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text') || ''
  const lines = text.split(/[\n,]/).map(u => u.trim()).filter(u => u && isValidUrl(u))

  for (const url of lines) {
    if (!videoInfos.value.find(v => v.url === url)) {
      await addUrlByUrl(url)
    }
  }
}

// 通过URL添加（不重复输入）
async function addUrlByUrl(url: string) {
  if (!isValidUrl(url)) return

  const existingIndex = videoInfos.value.findIndex(v => v.url === url)
  if (existingIndex !== -1) return

  const newInfo: VideoInfo = {
    url,
    title: '加载中...',
    resolution: '-',
    fileSize: '-',
    loading: true,
  }
  videoInfos.value.push(newInfo)

  try {
    const info = await getVideoInfo(url, ytdlpQuality.value)
    const index = videoInfos.value.findIndex(v => v.url === url)
    if (index !== -1) {
      videoInfos.value[index] = {
        url,
        title: info.title || '未知标题',
        resolution: info.resolution || '-',
        fileSize: info.file_size || '-',
        loading: false,
      }
    }
  } catch (e: any) {
    const index = videoInfos.value.findIndex(v => v.url === url)
    if (index !== -1) {
      videoInfos.value[index].loading = false
      videoInfos.value[index].error = e.message || '获取失败'
    }
  }
}

// 移除 URL
function removeUrl(index: number) {
  videoInfos.value.splice(index, 1)
}

// 是否可以开始（所有URL都已获取到信息，且没有失败的）
const canStart = computed(() => {
  if (videoInfos.value.length === 0) return false
  // 检查是否所有视频都在加载中
  if (videoInfos.value.some(v => v.loading)) return false
  // 检查是否有失败的
  if (videoInfos.value.some(v => v.error)) return false
  return true
})

// 获取成功的任务列表
const successfulTasks = computed(() => {
  return videoInfos.value.filter(v => !v.loading && !v.error)
})

// 确认添加
async function handleConfirm() {
  if (!canStart.value) return

  isAdding.value = true
  try {
    const urls = successfulTasks.value.map(v => v.url)
    await addYtdlpTasks(urls, ytdlpQuality.value)

    // 创建简化版任务对象返回给父组件
    const tasks: YtdlpTask[] = successfulTasks.value.map(v => ({
      id: crypto.randomUUID(),
      url: v.url,
      title: v.title,
      progress: 0,
      speed: '',
      status: 'Pending' as YtdlpTaskStatus,
      message: '等待下载',
      created_at: new Date().toISOString(),
      resolution: v.resolution,
      file_size: v.fileSize,
    }))

    emit('confirm', tasks)
  } catch (e: any) {
    errorMessage.value = '添加任务失败: ' + (e.message || e)
  } finally {
    isAdding.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-fade">
      <div v-show="visible" class="dialog-overlay" @click.self="$emit('close')">
        <div class="dialog">
          <div class="dialog-header">
            <h3>添加下载任务</h3>
            <button @click="$emit('close')" class="close-btn">
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>

          <div class="dialog-body">
            <!-- 输入框 -->
            <div class="input-group">
              <input
                type="text"
                v-model="urlInput"
                @keyup.enter="addUrl"
                @paste="handlePaste"
                placeholder="输入视频链接后按回车添加"
                class="url-input"
              />
              <button @click="addUrl" class="add-btn-small">添加</button>
            </div>

            <!-- 错误提示 -->
            <div v-if="errorMessage" class="error-message">{{ errorMessage }}</div>

            <!-- 视频信息列表 -->
            <div v-if="videoInfos.length > 0" class="video-list">
              <div class="list-header">
                <span>已添加 {{ videoInfos.length }} 个任务</span>
                <button @click="videoInfos = []" class="clear-all">清空</button>
              </div>
              <div class="list-body">
                <div v-for="(video, index) in videoInfos" :key="video.url" class="video-item">
                  <div class="video-info">
                    <div v-if="video.loading" class="loading-info">
                      <div class="spinner-xs"></div>
                      <span>获取中...</span>
                    </div>
                    <template v-else-if="video.error">
                      <span class="error-text">{{ video.error }}</span>
                    </template>
                    <template v-else>
                      <div class="video-meta">
                        <span class="video-title">{{ video.title }}</span>
                        <div class="video-stats">
                          <span class="stat-item">{{ video.resolution }}</span>
                          <span class="stat-item">{{ video.fileSize }}</span>
                        </div>
                      </div>
                    </template>
                  </div>
                  <button @click="removeUrl(index)" class="remove-url">
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <line x1="18" y1="6" x2="6" y2="18"></line>
                      <line x1="6" y1="6" x2="18" y2="18"></line>
                    </svg>
                  </button>
                </div>
              </div>
            </div>

            <!-- 提示 -->
            <div v-else class="url-tip">
              支持粘贴多个链接（换行或逗号分隔）
            </div>
          </div>

          <div class="dialog-footer">
            <button @click="$emit('close')" class="btn btn-secondary">取消</button>
            <button
              @click="handleConfirm"
              class="btn btn-primary"
              :disabled="!canStart || isAdding"
            >
              <template v-if="isAdding">
                <div class="spinner-xs"></div>
                添加中...
              </template>
              <template v-else>
                开始添加 {{ successfulTasks.length > 0 ? `(${successfulTasks.length}个)` : '' }}
              </template>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
}

.dialog {
  width: 90%;
  max-width: 600px;
  background: #fff;
  border-radius: 16px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.2);
  overflow: hidden;
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 20px 24px;
  border-bottom: 1px solid #f0f0f0;
}

.dialog-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #1a1a2e;
}

.close-btn {
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}

.close-btn:hover {
  color: #1a1a2e;
  background: #f5f6f8;
}

.dialog-body {
  padding: 24px;
}

.input-group {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

.url-input {
  flex: 1;
  padding: 12px 16px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  font-size: 14px;
  transition: all 0.2s;
}

.url-input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.add-btn-small {
  padding: 12px 20px;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  white-space: nowrap;
}

.add-btn-small:hover {
  background: #5a67d8;
}

.error-message {
  padding: 10px 14px;
  background: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: 8px;
  color: #dc2626;
  font-size: 13px;
  margin-bottom: 16px;
}

.video-list {
  border: 1px solid #f0f0f0;
  border-radius: 10px;
  overflow: hidden;
}

.list-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background: #fafbfc;
  border-bottom: 1px solid #f0f0f0;
  font-size: 13px;
  color: #64748b;
}

.clear-all {
  background: transparent;
  border: none;
  color: #667eea;
  font-size: 12px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  transition: all 0.2s;
}

.clear-all:hover {
  background: #eef2ff;
}

.list-body {
  max-height: 300px;
  overflow-y: auto;
}

.video-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  border-bottom: 1px solid #f5f5f5;
}

.video-item:last-child {
  border-bottom: none;
}

.video-info {
  flex: 1;
  min-width: 0;
}

.loading-info {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #94a3b8;
  font-size: 13px;
}

.error-text {
  color: #dc2626;
  font-size: 13px;
}

.video-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.video-title {
  font-size: 13px;
  color: #1a1a2e;
  font-weight: 500;
  word-break: break-all;
  line-height: 1.4;
}

.video-stats {
  display: flex;
  gap: 12px;
}

.stat-item {
  font-size: 12px;
  color: #64748b;
  background: #f5f6f8;
  padding: 2px 8px;
  border-radius: 4px;
}

.remove-url {
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
  flex-shrink: 0;
  margin-left: 12px;
}

.remove-url:hover {
  color: #dc2626;
  background: #fee2e2;
}

.url-tip {
  text-align: center;
  color: #94a3b8;
  font-size: 13px;
  padding: 20px;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 16px 24px;
  border-top: 1px solid #f0f0f0;
  background: #fafbfc;
}

.btn {
  padding: 10px 20px;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  gap: 8px;
}

.btn-secondary {
  background: #f5f6f8;
  color: #64748b;
  border: 1px solid #e5e7eb;
}

.btn-secondary:hover {
  background: #e5e7eb;
  color: #1a1a2e;
}

.btn-primary {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
}

.btn-primary:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.spinner-xs {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-top-color: white;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* 过渡动画 */
.dialog-fade-enter-active,
.dialog-fade-leave-active {
  transition: all 0.3s ease;
}

.dialog-fade-enter-from,
.dialog-fade-leave-to {
  opacity: 0;
}

.dialog-fade-enter-from .dialog,
.dialog-fade-leave-to .dialog {
  transform: scale(0.95) translateY(10px);
}
</style>
