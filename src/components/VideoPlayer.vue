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
      })

      hlsRef.value.loadSource(props.src)
      hlsRef.value.attachMedia(videoRef.value)

      hlsRef.value.on(Hls.Events.MANIFEST_PARSED, () => {
        isLoading.value = false
        videoRef.value?.play().catch(() => {
          // 自动播放被阻止，等待用户交互
        })
      })

      hlsRef.value.on(Hls.Events.ERROR, (event, data) => {
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

function togglePlay() {
  if (!videoRef.value) return
  if (videoRef.value.paused) {
    videoRef.value.play()
  } else {
    videoRef.value.pause()
  }
}

watch(() => props.visible, async (visible) => {
  if (visible) {
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

onMounted(() => {
  // 如果组件挂载时已经可见，初始化播放器
  if (props.visible) {
    nextTick(() => {
      setTimeout(initPlayer, 100)
    })
  }
})

onUnmounted(() => {
  destroyHls()
})
</script>

<template>
  <Transition name="fade">
    <div v-if="visible" class="player-overlay" @click.self="handleClose">
      <div class="player-container">
        <div class="player-header">
          <span class="player-title">{{ title }}</span>
          <button class="close-btn" @click="handleClose">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        </div>

        <div class="video-wrapper">
          <video
            ref="videoRef"
            class="video-element"
            controls
            playsinline
            @click="togglePlay"
          ></video>

          <div v-if="isLoading" class="loading-overlay">
            <div class="spinner"></div>
            <span>加载中...</span>
          </div>

          <div v-if="error" class="error-overlay">
            <span>{{ error }}</span>
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
  background: rgba(0, 0, 0, 0.85);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}

.player-container {
  width: 90%;
  max-width: 1000px;
  background: #1a1a2e;
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

.player-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  background: #16162a;
  border-bottom: 1px solid #2d2d44;
}

.player-title {
  color: #fff;
  font-size: 16px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.close-btn {
  padding: 8px;
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  border-radius: 6px;
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
  width: 100%;
  aspect-ratio: 16 / 9;
  background: #000;
}

.video-element {
  width: 100%;
  height: 100%;
  object-fit: contain;
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
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
