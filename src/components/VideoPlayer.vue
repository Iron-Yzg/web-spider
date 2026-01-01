<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import Hls from 'hls.js'

interface Props {
  visible: boolean
  src: string
  title: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
}>()

const videoRef = ref<HTMLVideoElement | null>(null)
const hlsRef = ref<Hls | null>(null)
const isLoading = ref(true)
const error = ref<string | null>(null)
const isFullscreen = ref(false)

// 根据窗口大小计算初始尺寸 (窗口的 35% 宽，16:9 比例)
function getInitialSize() {
  const width = Math.min(900, Math.floor(window.innerWidth * 0.5))
  const height = Math.floor(width * 9 / 16)
  return { width, height }
}

// 计算居中位置
function getCenteredPosition(width: number, height: number) {
  return {
    x: Math.max(0, Math.floor((window.innerWidth - width) / 2)),
    y: Math.max(0, Math.floor((window.innerHeight - height) / 2))
  }
}

// 可拖拽状态
const playerPosition = ref({ x: 0, y: 0 })
const playerSize = ref({ width: 0, height: 0 })
const isDragging = ref(false)
const dragOffset = ref({ x: 0, y: 0 })

// 调整大小的状态
const isResizing = ref(false)
const resizeDirection = ref('')
const resizeStartPos = ref({ x: 0, y: 0 })
const resizeStartSize = ref({ width: 0, height: 0 })
const resizeStartPosition = ref({ x: 0, y: 0 })

// 全屏恢复状态
const savedPosition = ref({ x: 0, y: 0 })
const savedSize = ref({ width: 0, height: 0 })

// 初始化播放器位置和大小
function initPlayerPosition() {
  const size = getInitialSize()
  playerSize.value = size
  playerPosition.value = getCenteredPosition(size.width, size.height)
}

// 开始拖拽
function startDrag(event: MouseEvent) {
  isDragging.value = true
  dragOffset.value = {
    x: event.clientX - playerPosition.value.x,
    y: event.clientY - playerPosition.value.y
  }
  document.addEventListener('mousemove', onDrag)
  document.addEventListener('mouseup', stopDrag)
}

// 拖拽中
function onDrag(event: MouseEvent) {
  if (!isDragging.value) return
  playerPosition.value = {
    x: event.clientX - dragOffset.value.x,
    y: event.clientY - dragOffset.value.y
  }
}

// 停止拖拽
function stopDrag() {
  isDragging.value = false
  document.removeEventListener('mousemove', onDrag)
  document.removeEventListener('mouseup', stopDrag)
}

// 开始调整大小
function startResize(event: MouseEvent, direction: string) {
  if (isFullscreen.value) return
  event.preventDefault()
  event.stopPropagation()

  isResizing.value = true
  resizeDirection.value = direction
  resizeStartPos.value = { x: event.clientX, y: event.clientY }
  resizeStartSize.value = { ...playerSize.value }
  resizeStartPosition.value = { ...playerPosition.value }

  document.addEventListener('mousemove', onResize)
  document.addEventListener('mouseup', stopResize)
}

// 调整大小中
function onResize(event: MouseEvent) {
  if (!isResizing.value) return

  const dx = event.clientX - resizeStartPos.value.x
  const dy = event.clientY - resizeStartPos.value.y
  const minWidth = 400
  const minHeight = 225
  const maxWidth = window.innerWidth - playerPosition.value.x
  const maxHeight = window.innerHeight - playerPosition.value.y

  let newWidth = resizeStartSize.value.width
  let newHeight = resizeStartSize.value.height
  let newX = resizeStartPosition.value.x
  let newY = resizeStartPosition.value.y

  const direction = resizeDirection.value

  if (direction.includes('e')) {
    newWidth = Math.max(minWidth, Math.min(maxWidth, resizeStartSize.value.width + dx))
  }
  if (direction.includes('w')) {
    const newLeft = resizeStartPosition.value.x + dx
    if (newLeft >= 0 && newLeft + minWidth <= window.innerWidth) {
      newX = newLeft
      newWidth = Math.max(minWidth, Math.min(window.innerWidth - newX, resizeStartSize.value.width - dx))
    }
  }
  if (direction.includes('s')) {
    newHeight = Math.max(minHeight, Math.min(maxHeight, resizeStartSize.value.height + dy))
  }
  if (direction.includes('n')) {
    const newTop = resizeStartPosition.value.y + dy
    if (newTop >= 0 && newTop + minHeight <= window.innerHeight) {
      newY = newTop
      newHeight = Math.max(minHeight, Math.min(window.innerHeight - newY, resizeStartSize.value.height - dy))
    }
  }

  playerSize.value = { width: newWidth, height: newHeight }
  playerPosition.value = { x: newX, y: newY }
}

// 停止调整大小
function stopResize() {
  isResizing.value = false
  resizeDirection.value = ''
  document.removeEventListener('mousemove', onResize)
  document.removeEventListener('mouseup', stopResize)
}

// 全屏切换 - 占满应用窗口
function toggleFullscreen() {
  if (isFullscreen.value) {
    // 退出全屏 - 恢复之前的位置和大小
    playerPosition.value = { ...savedPosition.value }
    playerSize.value = { ...savedSize.value }
    isFullscreen.value = false
  } else {
    // 进入全屏 - 保存当前状态
    savedPosition.value = { ...playerPosition.value }
    savedSize.value = { ...playerSize.value }
    // 设置为全屏大小（应用窗口内，占满底部）
    playerPosition.value = { x: 0, y: 0 }
    playerSize.value = { width: window.innerWidth, height: window.innerHeight }
    isFullscreen.value = true
  }
}

function initPlayer() {
  // 确保 video 元素存在
  if (!videoRef.value) {
    console.warn('Video element not found, retrying...')
    setTimeout(initPlayer, 100)
    return
  }
  if (!props.src) return

  isLoading.value = true
  error.value = null

  // 如果是 M3U8 链接，使用 hls.js
  if (props.src.endsWith('.m3u8') || props.src.includes('.m3u8')) {
    if (Hls.isSupported()) {
      destroyHls()

      hlsRef.value = new Hls({
        enableWorker: true,
        lowLatencyMode: true,
        backBufferLength: 90,
        startPosition: -1,
        maxBufferLength: 30,
        maxMaxBufferLength: 600,
      })

      // 确保加载完成后立即播放
      hlsRef.value.on(Hls.Events.MANIFEST_PARSED, () => {
        isLoading.value = false
        const playPromise = videoRef.value?.play()
        if (playPromise) {
          playPromise.catch(() => {
            // 自动播放被阻止，等待用户交互
          })
        }
      })

      // 监听错误恢复
      hlsRef.value.on(Hls.Events.ERROR, (_event, data) => {
        if (data.fatal) {
          switch (data.type) {
            case Hls.ErrorTypes.NETWORK_ERROR:
              error.value = '网络错误，正在尝试恢复...'
              hlsRef.value?.startLoad()
              break
            case Hls.ErrorTypes.MEDIA_ERROR:
              error.value = '媒体错误，正在尝试恢复...'
              hlsRef.value?.recoverMediaError()
              break
            default:
              error.value = `播放错误: ${data.type}`
              destroyHls()
              break
          }
        }
      })

      hlsRef.value.loadSource(props.src)
      hlsRef.value.attachMedia(videoRef.value)
    } else if (videoRef.value.canPlayType('application/vnd.apple.mpegurl')) {
      // Safari 原生支持 HLS
      videoRef.value.src = props.src
      videoRef.value.addEventListener('loadedmetadata', () => {
        isLoading.value = false
        videoRef.value?.play().catch(() => {})
      })
    } else {
      error.value = '您的浏览器不支持播放 M3U8 格式'
    }
  } else {
    // 普通视频链接
    videoRef.value.src = props.src
    videoRef.value.addEventListener('loadedmetadata', () => {
      isLoading.value = false
      videoRef.value?.play().catch(() => {})
    })
  }
}

function destroyHls() {
  if (hlsRef.value) {
    hlsRef.value.destroy()
    hlsRef.value = null
  }
}

function handleClose() {
  // 确保在播放器和 DOM 都存在时清理
  if (videoRef.value) {
    try {
      videoRef.value.pause()
      videoRef.value.src = ''
    } catch (e) {
      // 忽略可能的错误
    }
  }
  destroyHls()
  emit('close')
}

// 播放状态变化处理 (用于同步 HLS 状态)
function onPlayStateChange() {
  // 视频元素的 controls 属性会自动处理播放/暂停
  // 这里可以添加自定义逻辑
}

// HLS 进度条拖动处理
function handleSeeking() {
  if (hlsRef.value && videoRef.value) {
    // HLS.js 在 seek 时会自动处理，不需要额外操作
  }
}

function handleSeeked() {
  // 进度条拖动完成后，确保播放继续
  if (videoRef.value && videoRef.value.paused) {
    videoRef.value.play().catch(() => {})
  }
}

onMounted(() => {
  // 如果组件挂载时已经可见，初始化播放器
  if (props.visible) {
    initPlayerPosition()
    nextTick(() => {
      setTimeout(initPlayer, 100)
    })
  }
  // 监听 ESC 键退出播放器全屏
  document.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  destroyHls()
  document.removeEventListener('keydown', handleKeydown)
})

// 处理键盘事件
function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && isFullscreen.value) {
    toggleFullscreen()
  }
}

watch(() => props.visible, async (visible) => {
  if (visible) {
    // 每次打开时计算位置和大小（基于当前窗口）
    initPlayerPosition()
    // 等待 DOM 更新
    await nextTick()
    // 再等待一下确保 video 元素存在
    await new Promise(resolve => setTimeout(resolve, 100))
    initPlayer()
  } else {
    destroyHls()
  }
})

watch(() => props.src, () => {
  if (props.visible) {
    initPlayer()
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
        <!-- 拖拽标题栏 -->
        <div class="player-header" @mousedown="startDrag">
          <span class="player-title">{{ title }}</span>
          <div class="header-actions">
            <button class="action-btn" @click="toggleFullscreen" :title="isFullscreen ? '退出全屏' : '全屏'">
              <svg v-if="!isFullscreen" xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="15 3 21 3 21 9"></polyline>
                <polyline points="9 21 3 21 3 15"></polyline>
                <line x1="21" y1="3" x2="14" y2="10"></line>
                <line x1="3" y1="21" x2="10" y2="14"></line>
              </svg>
              <svg v-else xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="4 14 10 14 10 20"></polyline>
                <polyline points="20 10 14 10 14 4"></polyline>
                <line x1="14" y1="10" x2="21" y2="3"></line>
                <line x1="3" y1="21" x2="10" y2="14"></line>
              </svg>
            </button>
            <button class="close-btn" @click="handleClose">
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>
        </div>

        <div class="video-wrapper">
          <video
            ref="videoRef"
            class="video-element"
            controls
            playsinline
            @seeking="handleSeeking"
            @seeked="handleSeeked"
            @play="onPlayStateChange"
            @pause="onPlayStateChange"
          ></video>

          <div v-if="isLoading" class="loading-overlay">
            <div class="spinner"></div>
            <span>加载中...</span>
          </div>

          <div v-if="error" class="error-overlay">
            <span>{{ error }}</span>
          </div>
        </div>

        <!-- 调整大小的手柄（非全屏时显示） -->
        <template v-if="!isFullscreen">
          <div class="resize-handle resize-e" @mousedown="startResize($event, 'e')"></div>
          <div class="resize-handle resize-s" @mousedown="startResize($event, 's')"></div>
          <div class="resize-handle resize-se" @mousedown="startResize($event, 'se')"></div>
          <div class="resize-handle resize-sw" @mousedown="startResize($event, 'sw')"></div>
        </template>
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
  user-select: none;
}

.player-container.is-fullscreen {
  border-radius: 0;
}

.player-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background: #16162a;
  border-bottom: 1px solid #2d2d44;
  cursor: move;
  user-select: none;
}

.player-header:hover {
  background: #1a1a35;
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
  gap: 4px;
}

.action-btn {
  padding: 6px;
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
}

.close-btn {
  padding: 6px;
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  background: #2d2d44;
  color: #fff;
}

.video-wrapper {
  position: relative;
  flex: 1;
  background: #000;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.video-element {
  width: 100%;
  height: 100%;
  object-fit: contain;
  background: #000;
  display: block;
}

/* 视频控件样式 */
.video-element::-webkit-media-controls {
  background: transparent !important;
}

.video-element::-webkit-media-controls-enclosure {
  background: transparent !important;
  opacity: 1 !important;
}

.video-element::-webkit-media-controls-panel {
  background: transparent !important;
}

/* 移除 hover 时的蓝色遮罩 */
.video-element::-webkit-media-controls-enclosure:hover {
  background: transparent !important;
}

.video-element::-webkit-media-controls-play-button,
.video-element::-webkit-media-controls-mute-button,
.video-element::-webkit-media-controls-volume-slider {
  background: transparent !important;
  border-radius: 4px !important;
}

.video-element::-webkit-media-controls-timeline {
  background: rgba(255, 255, 255, 0.2) !important;
}

/* 移除默认的播放按钮遮罩 */
.video-element::-webkit-media-controls-start-overlays {
  background: transparent !important;
}

.loading-overlay,
.error-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  background: rgba(0, 0, 0, 0.7);
}

.loading-overlay span {
  color: #94a3b8;
  font-size: 14px;
}

.error-overlay span {
  color: #f87171;
  font-size: 14px;
}

.spinner {
  width: 40px;
  height: 40px;
  border: 3px solid #2d2d44;
  border-top-color: #667eea;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* 过渡动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* 调整大小的手柄 */
.resize-handle {
  position: absolute;
  z-index: 10;
  user-select: none;
}

.resize-e {
  right: 0;
  top: 0;
  width: 6px;
  height: 100%;
  cursor: e-resize;
}

.resize-s {
  bottom: 0;
  left: 0;
  width: 100%;
  height: 6px;
  cursor: s-resize;
}

.resize-se {
  right: 0;
  bottom: 0;
  width: 16px;
  height: 16px;
  cursor: se-resize;
  clip-path: polygon(100% 0, 0 100%, 100% 100%);
  background: rgba(255, 255, 255, 0.1);
}

.resize-sw {
  left: 0;
  bottom: 0;
  width: 16px;
  height: 16px;
  cursor: sw-resize;
  clip-path: polygon(0 0, 100% 100%, 0 100%);
  background: rgba(255, 255, 255, 0.1);
}

.resize-e:hover,
.resize-s:hover,
.resize-se:hover,
.resize-sw:hover {
  background: rgba(255, 255, 255, 0.2);
}
</style>
