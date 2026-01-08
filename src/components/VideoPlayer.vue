<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick, onBeforeUnmount } from 'vue'
import Artplayer from 'artplayer'
import Hls from 'hls.js'

interface VideoItem {
  id: string
  name: string
  m3u8_url: string
  status: string
  created_at: string
  downloaded_at?: string | null
  scrape_id: string
  website_name: string
}

interface Props {
  visible: boolean
  src: string
  title: string
  playlist?: VideoItem[]
  currentIndex?: number
}

const props = withDefaults(defineProps<Props>(), {
  playlist: () => [],
  currentIndex: 0
})

const emit = defineEmits<{
  close: []
  'play-next': [nextIndex: number]
  'delete-current': []
}>()

const containerRef = ref<HTMLDivElement | null>(null)
const artRef = ref<Artplayer | null>(null)
const isLoading = ref(true)
const error = ref<string | null>(null)

// 计算是否有下一个视频
const hasNextVideo = ref(false)

function updateHasNextVideo() {
  hasNextVideo.value = props.playlist.length > 0 && props.currentIndex < props.playlist.length - 1
}

// 播放下一个视频
function playNextVideo() {
  if (hasNextVideo.value) {
    const nextIndex = props.currentIndex + 1
    emit('play-next', nextIndex)
  }
}

// 删除当前视频
function deleteCurrentVideo() {
  emit('delete-current')
}

// 根据窗口大小计算初始尺寸 (窗口的 80% 宽，16:9 比例)
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

// 可拖拽状态
const playerPosition = ref({ x: 0, y: 0 })
const playerSize = ref({ width: 0, height: 0 })
const isDragging = ref(false)
const dragOffset = ref({ x: 0, y: 0 })

// 全屏恢复状态
const savedPosition = ref({ x: 0, y: 0 })
const savedSize = ref({ width: 0, height: 0 })
const isFullscreen = ref(false)

// 初始化播放器位置和大小
function initPlayerPosition() {
  const size = getInitialSize()
  playerSize.value = size
  playerPosition.value = getCenteredPosition(size.width, size.height)
}

// 创建 ArtPlayer 实例
function createPlayer() {
  if (!containerRef.value || !props.src) return

  // 销毁旧的播放器
  destroyPlayer()

  const isM3U8 = props.src.endsWith('.m3u8') || props.src.includes('.m3u8')

  const art = new Artplayer({
    container: containerRef.value,
    url: props.src,
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
      crossOrigin: 'anonymous',
    },
  }) as Artplayer

  // 设置标题
  art.title = props.title

  // HLS 特殊处理
  if (isM3U8 && Hls.isSupported()) {
    const hls = new Hls({
      enableWorker: true,
      lowLatencyMode: true,
      backBufferLength: 90,
    })

    hls.loadSource(props.src)
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
      // 进入全屏
      savedPosition.value = { ...playerPosition.value }
      savedSize.value = { ...playerSize.value }
      playerPosition.value = { x: 0, y: 0 }
      playerSize.value = { width: window.innerWidth, height: window.innerHeight }
    } else {
      // 退出全屏
      playerPosition.value = { ...savedPosition.value }
      playerSize.value = { ...savedSize.value }
      isFullscreen.value = false
    }
  })

  artRef.value = art
}

// 销毁播放器
function destroyPlayer() {
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

onMounted(() => {
  initPlayerPosition()
  updateHasNextVideo()
})

onBeforeUnmount(() => {
  destroyPlayer()
})

onUnmounted(() => {
  destroyPlayer()
})

watch(() => props.visible, async (visible) => {
  if (visible) {
    initPlayerPosition()
    updateHasNextVideo()
    await nextTick()
    await new Promise(resolve => setTimeout(resolve, 50))
    createPlayer()
  } else {
    destroyPlayer()
  }
})

watch(() => [props.playlist, props.currentIndex], () => {
  updateHasNextVideo()
}, { deep: true })

watch(() => props.src, async (newSrc) => {
  if (props.visible && newSrc) {
    await nextTick()
    await new Promise(resolve => setTimeout(resolve, 50))
    createPlayer()
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
        <!-- 拖拽标题栏（非全屏时显示，悬停显示） -->
        <div v-if="!isFullscreen" class="player-header" @mousedown="startDrag">
          <span class="player-title">{{ title }}</span>
          <div class="header-actions">
            <!-- 删除当前视频按钮 -->
            <button
              class="action-btn delete-btn"
              @click.stop="deleteCurrentVideo"
              title="删除当前视频"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="3 6 5 6 21 6"></polyline>
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
                <line x1="10" y1="11" x2="10" y2="17"></line>
                <line x1="14" y1="11" x2="14" y2="17"></line>
              </svg>
            </button>
            <!-- 播放下一个按钮 -->
            <button
              v-if="hasNextVideo"
              class="action-btn next-btn"
              @click.stop="playNextVideo"
              title="播放下一个"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polygon points="5 3 19 12 5 21 5 3"></polygon>
                <line x1="19" y1="12" x2="9" y2="12"></line>
                <line x1="19" y1="12" x2="9" y2="5"></line>
                <line x1="19" y1="12" x2="9" y2="19"></line>
              </svg>
              <span class="next-text">下一个</span>
            </button>
            <button class="close-btn" @click="handleClose" title="关闭">
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>
        </div>

        <!-- ArtPlayer 容器 -->
        <div ref="containerRef" class="artplayer-container"></div>

        <!-- 错误提示 -->
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

/* 标题栏默认隐藏，鼠标悬停显示 */
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
  padding: 6px 12px;
  background: transparent;
  border: none;
  color: #fff;
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  gap: 4px;
}

.player-container:hover .action-btn {
  opacity: 1;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.1);
}

.next-btn {
  background: rgba(102, 126, 234, 0.2);
  border: 1px solid rgba(102, 126, 234, 0.5);
}

.next-btn:hover {
  background: rgba(102, 126, 234, 0.4);
}

.delete-btn {
  background: rgba(239, 68, 68, 0.2);
  border: 1px solid rgba(239, 68, 68, 0.5);
}

.delete-btn:hover {
  background: rgba(239, 68, 68, 0.4);
}

.next-text {
  font-size: 12px;
}

.close-btn {
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

.player-container:hover .close-btn {
  opacity: 1;
}

.close-btn:hover {
  background: rgba(255, 255, 255, 0.1);
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

/* 过渡动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* ArtPlayer 自定义样式覆盖 */
:deep(.artplayer) {
  --art-theme: #1a1a2e;
  --art-primary: #667eea;
}

/* 修复播放按钮显示 */
:deep(.art-icon-play svg),
:deep(.art-icon-pause svg),
:deep(.art-icon-fullscreen svg),
:deep(.art-icon-fullscreenWeb svg),
:deep(.art-icon-settings svg),
:deep(.art-icon-zoomin svg),
:deep(.art-icon-zoomout svg),
:deep(.art-icon-volume svg),
:deep(.art-icon-volumeClose svg),
:deep(.art-icon-speed svg),
:deep(.art-icon-aspactRatio svg),
:deep(.art-icon-pip svg),
:deep(.art-icon-loop svg),
:deep(.art-icon-flipped svg) {
  fill: white !important;
  stroke: white !important;
}

/* 控制栏按钮颜色 */
:deep(.art-control-btn) {
  color: white !important;
}

/* 进度条颜色 */
:deep(.art-progress-loaded) {
  background: rgba(255, 255, 255, 0.3) !important;
}

:deep(.art-progress-played) {
  background: #667eea !important;
}

/* 时间文字颜色 */
:deep(.art-time) {
  color: white !important;
}
</style>
