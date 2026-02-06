<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick, onBeforeUnmount, computed } from 'vue'
import Artplayer from 'artplayer'
import Hls from 'hls.js'

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

// 播放下一个视频
function handlePlayNext() {
  if (hasNextVideo.value) {
    const nextIndex = props.currentIndex + 1
    emit('playNext', nextIndex)
  }
}

// 删除当前视频
function handleDelete() {
  emit('deleteCurrent')
}

const containerRef = ref<HTMLDivElement | null>(null)
const artRef = ref<Artplayer | null>(null)
const hlsRef = ref<Hls | null>(null)
const isLoading = ref(true)
const error = ref<string | null>(null)

// 可拖拽状态
const playerPosition = ref({ x: 0, y: 0 })
const playerSize = ref({ width: 0, height: 0 })
const isDragging = ref(false)
const dragOffset = ref({ x: 0, y: 0 })

// 全屏恢复状态
const savedPosition = ref({ x: 0, y: 0 })
const savedSize = ref({ width: 0, height: 0 })
const isFullscreen = ref(false)

// 防止重复创建
const isCreating = ref(false)

// 根据窗口大小计算初始尺寸
function getInitialSize() {
  const width = Math.min(900, Math.floor(window.innerWidth * 0.8))
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

// 初始化播放器位置和大小
function initPlayerPosition() {
  const size = getInitialSize()
  playerSize.value = size
  playerPosition.value = getCenteredPosition(size.width, size.height)
}

// 开始拖拽
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

// 创建 ArtPlayer 实例
async function createPlayer() {
  if (!containerRef.value || !props.src) return

  console.log('[VideoPlayer] 创建播放器:', props.src)

  // 销毁旧的播放器
  destroyPlayer()
  await new Promise(resolve => setTimeout(resolve, 100))

  const currentUrl = props.src
  console.log('[VideoPlayer] 使用 URL:', currentUrl)

  // 假设传入的 URL 已经是正确格式（asset:// 或 http://）
  const isM3U8 = currentUrl.endsWith('.m3u8') || currentUrl.includes('.m3u8')

  const art = new Artplayer({
    container: containerRef.value,
    url: currentUrl,
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

  // HLS 特殊处理
  if (isM3U8 && Hls.isSupported()) {
    const hls = new Hls({
      enableWorker: false,
      lowLatencyMode: false,
      backBufferLength: 90,
      maxBufferLength: 30,
      maxMaxBufferLength: 60,
    })
    hlsRef.value = hls

    hls.loadSource(currentUrl)
    hls.attachMedia(art.video)

    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      isLoading.value = false
    })

    hls.on(Hls.Events.ERROR, (_event, data) => {
      if (data.fatal) {
        switch (data.type) {
          case Hls.ErrorTypes.NETWORK_ERROR:
            error.value = '网络错误'
            hls.startLoad()
            break
          case Hls.ErrorTypes.MEDIA_ERROR:
            error.value = '媒体错误'
            hls.recoverMediaError()
            break
          default:
            error.value = `播放错误: ${data.type}`
            hlsRef.value = null
            hls.destroy()
            break
        }
      }
    })

    art.once('ready', () => {
      isLoading.value = false
    })
  } else {
    art.once('ready', () => {
      isLoading.value = false
    })
  }

  art.on('error', (err) => {
    error.value = '播放出错'
    console.error('ArtPlayer error:', err)
  })

  art.on('play', () => {
    isLoading.value = false
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
      isFullscreen.value = false
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
}

// 关闭播放器
function handleClose() {
  destroyPlayer()
  emit('close')
}

onMounted(() => {
  initPlayerPosition()
})

onBeforeUnmount(() => {
  destroyPlayer()
})

onUnmounted(() => {
  destroyPlayer()
})

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
            <!-- 播放下一个按钮 -->
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
            <!-- 删除按钮 -->
            <button class="action-btn delete-btn" @click.stop="handleDelete" title="删除">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 6h18"></path>
                <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"></path>
                <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path>
              </svg>
            </button>
            <!-- 关闭按钮 -->
            <button class="action-btn" @click.stop="handleClose" title="关闭">
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>
        </div>

        <div ref="containerRef" class="artplayer-container"></div>

        <div v-if="error" class="error-overlay">
          <span>{{ error }}</span>
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
}

.error-overlay span {
  color: #f87171;
  font-size: 14px;
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
