<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick, onBeforeUnmount, computed } from 'vue'
import Artplayer from 'artplayer'
import Hls from 'hls.js'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'

interface Props {
  visible: boolean
  src: string
  title: string
  playlist?: any[]
  currentIndex?: number
  videoId?: string
}

const props = withDefaults(defineProps<Props>(), {
  visible: false,
  src: '',
  title: '',
  playlist: () => [],
  currentIndex: 0,
  videoId: ''
})

const emit = defineEmits<{
  close: []
  playNext: [index: number]
  deleteCurrent: []
}>()

// 计算是否有下一个视频
const hasNextVideo = computed(() => {
  return props.playlist && props.currentIndex < props.playlist.length - 1
})

// 播放控制
const isTranscoding = ref(false)
const transcodingProgress = ref(0)
const error = ref<string | null>(null)
const containerRef = ref<HTMLDivElement | null>(null)
const artRef = ref<Artplayer | null>(null)
const hlsRef = ref<Hls | null>(null)
const currentTranscodeSession = ref<string | null>(null)
const transcodeMode = ref<'remux' | 'transcode'>('remux') // 'remux'=解复用(快), 'transcode'=转码(慢)

// 可拖拽状态
const playerPosition = ref({ x: 0, y: 0 })
const playerSize = ref({ width: 0, height: 0 })
const isDragging = ref(false)
const dragOffset = ref({ x: 0, y: 0 })
const isFullscreen = ref(false)
const savedPosition = ref({ x: 0, y: 0 })
const savedSize = ref({ width: 0, height: 0 })

// 防止重复创建
const isCreating = ref(false)

// 需要转码的格式
const TRANSCODE_FORMATS = ['.mkv', '.avi', '.flv', '.wmv', '.rm', '.rmvb', '.ts', '.mpeg', '.mpg']

function normalizeMediaPath(input: string): string {
  let value = input.trim()
  value = value.replace(/\\\//g, '/')
  value = value.replace(/^"(.*)"$/, '$1')
  return value
}

function isHttpUrl(path: string): boolean {
  return /^https?:\/\//i.test(path)
}

// 检查是否需要转码（仅本地文件）
function needsTranscoding(url: string): boolean {
  const normalized = normalizeMediaPath(url)
  // 网络 URL 直接返回 false
  if (isHttpUrl(normalized)) {
    return false
  }
  // 本地文件检查扩展名
  const lowerUrl = normalized.toLowerCase()
  return TRANSCODE_FORMATS.some(ext => lowerUrl.endsWith(ext))
}

// 开始视频播放（自动选择解复用或转码）
async function startVideoPlayback(filePath: string): Promise<{ url: string; isTranscoding: boolean }> {
  try {
    isTranscoding.value = true
    transcodingProgress.value = 0
    error.value = null
    transcodeMode.value = 'remux' // 默认解复用

    console.log('[VideoPlayer] 开始视频播放:', filePath)

    // 调用后端，自动检测编码并选择解复用或转码
    const result = await invoke<[string, boolean]>('start_video_playback_cmd', {
      filePath,
      sessionId: props.videoId || Date.now().toString()
    })

    currentTranscodeSession.value = props.videoId || Date.now().toString()
    const [hlsUrl, needsTranscode] = result

    if (needsTranscode) {
      console.log('[VideoPlayer] 使用转码播放:', hlsUrl)
      transcodeMode.value = 'transcode'
    } else {
      console.log('[VideoPlayer] 使用解复用播放（极速）:', hlsUrl)
      transcodeMode.value = 'remux'
    }

    return { url: hlsUrl, isTranscoding: needsTranscode }
  } catch (e) {
    console.error('[VideoPlayer] 播放失败:', e)
    error.value = '视频播放失败: ' + (e as Error).message
    throw e
  } finally {
    isTranscoding.value = false
  }
}

// 停止转码
async function stopTranscoding() {
  if (currentTranscodeSession.value) {
    try {
      await invoke('stop_video_transcode', {
        sessionId: currentTranscodeSession.value
      })
      console.log('[VideoPlayer] 停止转码:', currentTranscodeSession.value)
    } catch (e) {
      console.error('[VideoPlayer] 停止转码失败:', e)
    }
    currentTranscodeSession.value = null
  }
}

// 使用系统播放器打开
async function openWithSystemPlayer() {
  try {
    const filePath = normalizeMediaPath(props.src)
    console.log('[VideoPlayer] 使用系统播放器打开:', filePath)
    await invoke('open_with_system_player', { filePath })
    // 关闭当前播放器
    handleClose()
  } catch (e) {
    console.error('[VideoPlayer] 打开系统播放器失败:', e)
    error.value = '打开系统播放器失败: ' + (e as Error).message
  }
}

// 初始化播放器位置和大小
function getInitialSize() {
  const width = Math.min(900, Math.floor(window.innerWidth * 0.8))
  const height = Math.floor(width * 9 / 16)
  return { width, height }
}

function getCenteredPosition(width: number, height: number) {
  return {
    x: Math.max(0, Math.floor((window.innerWidth - width) / 2)),
    y: Math.max(0, Math.floor((window.innerHeight - height) / 2))
  }
}

function initPlayerPosition() {
  const size = getInitialSize()
  playerSize.value = size
  playerPosition.value = getCenteredPosition(size.width, size.height)
}

// 拖拽控制
function startDrag(event: MouseEvent) {
  if (isFullscreen.value) return
  isDragging.value = true
  dragOffset.value = {
    x: event.clientX - playerPosition.value.x,
    y: event.clientY - playerPosition.value.y
  }
  document.addEventListener('mousemove', onDrag)
  document.addEventListener('mouseup', stopDrag)
}

function onDrag(event: MouseEvent) {
  if (!isDragging.value) return
  playerPosition.value = {
    x: event.clientX - dragOffset.value.x,
    y: event.clientY - dragOffset.value.y
  }
}

function stopDrag() {
  isDragging.value = false
  document.removeEventListener('mousemove', onDrag)
  document.removeEventListener('mouseup', stopDrag)
}

// 获取文件信息（大小等）
async function getFileInfo(filePath: string): Promise<{ size: number; isLarge: boolean }> {
  try {
    const stats = await invoke<{ size: number }>('get_file_stats', { path: filePath })
    const sizeGB = stats.size / (1024 * 1024 * 1024)
    return { size: stats.size, isLarge: sizeGB > 1 } // 大于 1GB 视为大文件
  } catch (e) {
    return { size: 0, isLarge: false }
  }
}

// 创建播放器
async function createPlayer() {
  if (!containerRef.value || !props.src) return

  console.log('[VideoPlayer] 创建播放器:', props.src)

  // 销毁旧的
  destroyPlayer()
  await new Promise(resolve => setTimeout(resolve, 100))

  const originalPath = normalizeMediaPath(props.src)  // 原始文件路径
  let playUrl = originalPath
  let isHls = originalPath.toLowerCase().includes('.m3u8')

  // 检查是否需要转码/解复用
  if (needsTranscoding(originalPath)) {
    try {
      // 使用新的播放API，自动选择解复用或转码
      const result = await startVideoPlayback(originalPath)
      // 后端返回的是 HTTP URL（http://127.0.0.1:port/playlist.m3u8），直接使用
      playUrl = result.url
      isHls = true

      // 如果是转码模式且文件较大，给出提示
      if (result.isTranscoding) {
        const fileInfo = await getFileInfo(originalPath)
        if (fileInfo.isLarge) {
          console.log('[VideoPlayer] 文件较大，使用转码模式')
          // 显示提示但继续播放
          transcodingProgress.value = 50 // 模拟进度
        }
      }
    } catch (e) {
      console.error('[VideoPlayer] 播放失败:', e)
      error.value = '无法播放此视频格式，建议使用系统播放器'
      return
    }
  } else if (isHttpUrl(originalPath)) {
    // 网络视频直接使用，不需要 convertFileSrc
    playUrl = originalPath
    console.log('[VideoPlayer] 网络视频，直接使用URL:', playUrl)
  } else {
    // 本地原生支持的格式，转换为 asset URL
    playUrl = convertFileSrc(originalPath)
  }

  console.log('[VideoPlayer] 使用播放地址:', playUrl, '是否HLS:', isHls)
  
  const art = new Artplayer({
    container: containerRef.value,
    url: playUrl,
    autoplay: true,
    muted: false,
    volume: 0.5,
    loop: false,
    flip: true,
    playbackRate: true,
    aspectRatio: true,
    setting: true,
    pip: true,
    fullscreen: true,
    fullscreenWeb: true,
    subtitleOffset: true,
    miniProgressBar: true,
    mutex: true,
    backdrop: true,
    playsInline: true,
    autoPlayback: true,
    lang: 'zh-cn',
    theme: '#1a1a2e',
    moreVideoAttr: {
      crossOrigin: 'anonymous'
    },
  }) as Artplayer
  
  art.title = props.title
  
  // HLS 处理
  if (isHls && Hls.isSupported()) {
    const hls = new Hls({
      enableWorker: false,
      lowLatencyMode: false,
      backBufferLength: 90,
      maxBufferLength: 60,
      maxMaxBufferLength: 120,
      maxBufferSize: 50 * 1000 * 1000, // 50MB buffer
    })
    hlsRef.value = hls
    
    hls.loadSource(playUrl)
    hls.attachMedia(art.video)
    
    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      console.log('[VideoPlayer] HLS manifest 加载完成')
    })
    
    hls.on(Hls.Events.ERROR, (_event, data) => {
      console.error('[VideoPlayer] HLS 错误:', data)
      if (data.fatal) {
        switch (data.type) {
          case Hls.ErrorTypes.NETWORK_ERROR:
            error.value = '网络错误，正在重试...'
            hls.startLoad()
            break
          case Hls.ErrorTypes.MEDIA_ERROR:
            error.value = '媒体错误，正在恢复...'
            hls.recoverMediaError()
            break
          default:
            error.value = `播放错误: ${data.type}`
            break
        }
      }
    })
  }
  
  // 事件监听
  art.on('ready', () => {
    console.log('[VideoPlayer] 播放器就绪')
  })
  
  art.on('error', (err) => {
    console.error('[VideoPlayer] 播放错误:', err)
    error.value = '播放出错，请重试'
  })
  
  art.on('fullscreen', (state) => {
    isFullscreen.value = state
    if (state) {
      savedPosition.value = { ...playerPosition.value }
      savedSize.value = { ...playerSize.value }
      playerPosition.value = { x: 0, y: 0 }
      playerSize.value = { width: window.innerWidth, height: window.innerHeight }
    } else {
      playerPosition.value = { ...savedPosition.value }
      playerSize.value = { ...savedSize.value }
    }
  })
  
  artRef.value = art
}

// 销毁播放器
function destroyPlayer() {
  if (hlsRef.value) {
    hlsRef.value.destroy()
    hlsRef.value = null
  }
  if (artRef.value) {
    artRef.value.destroy()
    artRef.value = null
  }
  // 停止转码
  stopTranscoding()
}

// 关闭播放器
function handleClose() {
  destroyPlayer()
  emit('close')
}

// 播放下一个
function handlePlayNext() {
  if (hasNextVideo.value) {
    emit('playNext', props.currentIndex + 1)
  }
}

// 删除当前
function handleDelete() {
  emit('deleteCurrent')
}

// 生命周期
onMounted(() => {
  initPlayerPosition()
})

onBeforeUnmount(() => {
  destroyPlayer()
})

onUnmounted(() => {
  destroyPlayer()
})

// 监听显示状态
watch(() => props.visible, async (visible) => {
  if (visible) {
    if (isCreating.value) return
    isCreating.value = true
    try {
      initPlayerPosition()
      await nextTick()
      await new Promise(resolve => setTimeout(resolve, 100))
      await createPlayer()
    } finally {
      isCreating.value = false
    }
  } else {
    destroyPlayer()
  }
})

// 监听源变化
watch(() => props.src, async (newSrc) => {
  if (props.visible && newSrc) {
    if (isCreating.value) return
    isCreating.value = true
    try {
      await nextTick()
      await new Promise(resolve => setTimeout(resolve, 100))
      await createPlayer()
    } finally {
      isCreating.value = false
    }
  }
})
</script>

<template>
  <Transition name="fade">
    <div v-if="visible" class="player-overlay" @click.self="handleClose">
      <div
        class="player-container"
        :class="{ 'is-fullscreen': isFullscreen }"
        :style="{
          left: playerPosition.x + 'px',
          top: playerPosition.y + 'px',
          width: playerSize.width + 'px',
          height: playerSize.height + 'px'
        }"
      >
        <div v-if="!isFullscreen" class="player-header" @mousedown="startDrag">
          <span class="player-title">{{ title }}</span>
          <div class="header-actions">
            <button
              v-if="hasNextVideo"
              class="action-btn"
              @click.stop="handlePlayNext"
              title="播放下一个"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polygon points="5 4 15 12 5 20 5 4"></polygon>
                <line x1="19" y1="5" x2="19" y2="19"></line>
              </svg>
            </button>
            <button class="action-btn delete-btn" @click.stop="handleDelete" title="删除">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 6h18"></path>
                <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"></path>
                <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path>
              </svg>
            </button>
            <button class="action-btn" @click.stop="handleClose" title="关闭">
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>
        </div>

        <!-- 转码中提示 -->
        <div v-if="isTranscoding" class="transcoding-overlay">
          <div class="transcoding-content">
            <div class="spinner"></div>
            <p v-if="transcodeMode === 'remux'">正在解复用视频...</p>
            <p v-else>正在转码视频格式...</p>
            <p v-if="transcodeMode === 'remux'" class="transcoding-hint">极速模式：不解码直接播放（约2-5秒）</p>
            <p v-else class="transcoding-hint">首次播放需要转码，请稍候（约10-30秒）</p>
          </div>
        </div>

        <div ref="containerRef" class="artplayer-container"></div>

        <div v-if="error && !isTranscoding" class="error-overlay">
          <div class="error-content">
            <span>{{ error }}</span>
            <button class="system-player-btn" @click="openWithSystemPlayer">
              使用系统播放器打开
            </button>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.player-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0);
  z-index: 9999;
  pointer-events: none;
}

.player-container {
  position: absolute;
  background: #1a1a2e;
  border-radius: 8px;
  overflow: hidden;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  display: flex;
  flex-direction: column;
  pointer-events: auto;
  min-width: 400px;
  min-height: 225px;
  transition: width 0.2s, height 0.2s, left 0.2s, top 0.2s;
}

.player-container.is-fullscreen {
  border-radius: 0;
}

.player-header {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background: linear-gradient(to bottom, rgba(22, 22, 42, 0.95), transparent);
  cursor: move;
  user-select: none;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.3s ease;
  z-index: 100;
}

.player-container:hover .player-header {
  opacity: 1;
}

.player-title {
  color: #fff;
  font-size: 14px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.action-btn {
  padding: 6px;
  background: transparent;
  border: none;
  color: #fff;
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
}

.player-container:hover .action-btn {
  opacity: 1;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.1);
}

.action-btn.delete-btn:hover {
  background: rgba(239, 68, 68, 0.3);
  color: #fca5a5;
}

.artplayer-container {
  flex: 1;
  min-height: 0;
}

.transcoding-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(26, 26, 46, 0.95);
  z-index: 50;
}

.transcoding-content {
  text-align: center;
  color: #fff;
}

.transcoding-content p {
  margin: 8px 0;
  font-size: 16px;
}

.transcoding-hint {
  font-size: 12px !important;
  color: #94a3b8;
}

.spinner {
  width: 50px;
  height: 50px;
  border: 4px solid rgba(255, 255, 255, 0.1);
  border-top-color: #667eea;
  border-radius: 50%;
  animation: spin 1s linear infinite;
  margin: 0 auto 16px;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.error-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.7);
  z-index: 60;
}

.error-overlay span {
  color: #f87171;
  font-size: 14px;
}

.error-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  text-align: center;
  padding: 20px;
}

.system-player-btn {
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

.system-player-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

:deep(.art-progress-played) {
  background: #667eea !important;
}

:deep(.art-time) {
  color: white !important;
}
</style>
