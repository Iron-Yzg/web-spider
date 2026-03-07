<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick, onBeforeUnmount, computed } from 'vue'
import Artplayer from 'artplayer'
import Hls from 'hls.js'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { ConvertTask, ConvertOptions } from '../types'
import { screenshotVideo, startConvert, stopConvert, openPath } from '../services/api'

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
const transcodeMode = ref<'remux' | 'transcode'>('remux')

// 可拖拽状态
const playerPosition = ref({ x: 0, y: 0 })
const playerSize = ref({ width: 0, height: 0 })
const isDragging = ref(false)
const dragOffset = ref({ x: 0, y: 0 })
const isFullscreen = ref(false)
const isTrueFullscreen = ref(false)
const savedPosition = ref({ x: 0, y: 0 })
const savedSize = ref({ width: 0, height: 0 })

// 防止重复创建
const isCreating = ref(false)

// ==================== 媒体工具相关状态 ====================
const showToolsPanel = ref(false)
const activeToolTab = ref<'screenshot' | 'convert' | 'audio' | 'gif'>('screenshot')
const toolMessage = ref<{ text: string; type: 'success' | 'error' | 'info' } | null>(null)
let toolMessageTimer: ReturnType<typeof setTimeout> | null = null

// 截图状态
const screenshotLoading = ref(false)

// 格式转换状态
const convertFormat = ref('mp4')
const convertLoading = ref(false)
const convertTask = ref<ConvertTask | null>(null)
let unlistenConvertProgress: (() => void) | null = null

// 音频提取状态
const audioFormat = ref('mp3')
const audioLoading = ref(false)
const audioTask = ref<ConvertTask | null>(null)

// GIF 导出状态
const gifStartTime = ref(0)
const gifEndTime = ref(5)
const gifFps = ref(10)
const gifResolution = ref('')
const gifLoading = ref(false)
const gifTask = ref<ConvertTask | null>(null)

// 判断当前视频是否为本地文件（工具仅对本地文件可用）
const isLocalFile = computed(() => {
  const src = props.src?.trim() || ''
  return src.length > 0 && !isHttpUrl(src)
})

// 获取当前播放时间
function getCurrentTime(): number {
  if (artRef.value) {
    return artRef.value.currentTime || 0
  }
  return 0
}

// 获取视频总时长
function getDuration(): number {
  if (artRef.value) {
    return artRef.value.duration || 0
  }
  return 0
}

// 显示工具提示消息
function showToolMessage(text: string, type: 'success' | 'error' | 'info' = 'info') {
  toolMessage.value = { text, type }
  if (toolMessageTimer) clearTimeout(toolMessageTimer)
  toolMessageTimer = setTimeout(() => {
    toolMessage.value = null
  }, 3000)
}

// ==================== 截图功能 ====================
async function handleScreenshot() {
  if (!isLocalFile.value) {
    showToolMessage('仅支持本地视频截图', 'error')
    return
  }
  const filePath = normalizeMediaPath(props.src)
  const timestamp = getCurrentTime()

  screenshotLoading.value = true
  try {
    const outputPath = await screenshotVideo(filePath, timestamp)
    showToolMessage(`截图已保存: ${outputPath.split('/').pop() || outputPath}`, 'success')
    // 打开截图所在文件夹
    const dir = outputPath.substring(0, outputPath.lastIndexOf('/'))
    if (dir) await openPath(dir)
  } catch (e) {
    showToolMessage('截图失败: ' + e, 'error')
  } finally {
    screenshotLoading.value = false
  }
}

// ==================== 格式转换功能 ====================
async function handleConvert() {
  if (!isLocalFile.value) {
    showToolMessage('仅支持本地视频转换', 'error')
    return
  }
  const filePath = normalizeMediaPath(props.src)

  convertLoading.value = true
  try {
    const options: ConvertOptions = {
      format: convertFormat.value,
      audio_only: false,
    }
    const task = await startConvert(filePath, null, options)
    convertTask.value = task
    if (task.status === 'Completed') {
      showToolMessage(`转换完成: ${task.output_path.split('/').pop()}`, 'success')
      convertLoading.value = false
    }
  } catch (e) {
    showToolMessage('转换失败: ' + e, 'error')
    convertLoading.value = false
  }
}

async function handleStopConvert() {
  if (convertTask.value) {
    try {
      await stopConvert(convertTask.value.id)
      showToolMessage('转换已停止', 'info')
    } catch (e) {
      console.error('停止转换失败:', e)
    }
    convertTask.value = null
    convertLoading.value = false
  }
}

// ==================== 音频提取功能 ====================
async function handleAudioExtract() {
  if (!isLocalFile.value) {
    showToolMessage('仅支持本地视频提取音频', 'error')
    return
  }
  const filePath = normalizeMediaPath(props.src)

  audioLoading.value = true
  try {
    const options: ConvertOptions = {
      format: audioFormat.value,
      audio_only: true,
    }
    const task = await startConvert(filePath, null, options)
    audioTask.value = task
    if (task.status === 'Completed') {
      showToolMessage(`音频提取完成: ${task.output_path.split('/').pop()}`, 'success')
      audioLoading.value = false
    }
  } catch (e) {
    showToolMessage('音频提取失败: ' + e, 'error')
    audioLoading.value = false
  }
}

async function handleStopAudio() {
  if (audioTask.value) {
    try {
      await stopConvert(audioTask.value.id)
      showToolMessage('音频提取已停止', 'info')
    } catch (e) {
      console.error('停止音频提取失败:', e)
    }
    audioTask.value = null
    audioLoading.value = false
  }
}

// ==================== GIF 导出功能 ====================
function initGifTimes() {
  const currentTime = getCurrentTime()
  const duration = getDuration()
  gifStartTime.value = Math.max(0, currentTime - 2)
  gifEndTime.value = Math.min(duration || currentTime + 5, currentTime + 5)
}

async function handleGifExport() {
  if (!isLocalFile.value) {
    showToolMessage('仅支持本地视频导出GIF', 'error')
    return
  }
  if (gifEndTime.value <= gifStartTime.value) {
    showToolMessage('结束时间必须大于开始时间', 'error')
    return
  }
  const filePath = normalizeMediaPath(props.src)

  gifLoading.value = true
  try {
    const options: ConvertOptions = {
      format: 'gif',
      audio_only: false,
      start_time: gifStartTime.value,
      end_time: gifEndTime.value,
      fps: gifFps.value,
      resolution: gifResolution.value || undefined,
    }
    const task = await startConvert(filePath, null, options)
    gifTask.value = task
    if (task.status === 'Completed') {
      showToolMessage(`GIF导出完成: ${task.output_path.split('/').pop()}`, 'success')
      gifLoading.value = false
    }
  } catch (e) {
    showToolMessage('GIF导出失败: ' + e, 'error')
    gifLoading.value = false
  }
}

async function handleStopGif() {
  if (gifTask.value) {
    try {
      await stopConvert(gifTask.value.id)
      showToolMessage('GIF导出已停止', 'info')
    } catch (e) {
      console.error('停止GIF导出失败:', e)
    }
    gifTask.value = null
    gifLoading.value = false
  }
}

// 切换工具面板
function toggleToolsPanel() {
  showToolsPanel.value = !showToolsPanel.value
  if (showToolsPanel.value) {
    initGifTimes()
  }
}

// ==================== Artplayer 自定义控制按钮图标 ====================

const ICON_SCREENSHOT = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"></path><circle cx="12" cy="13" r="4"></circle></svg>`

const ICON_TOOLS = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"></path></svg>`

const ICON_NEXT = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 4 15 12 5 20 5 4"></polygon><line x1="19" y1="5" x2="19" y2="19"></line></svg>`

const ICON_DELETE = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"></path><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"></path><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path></svg>`

const ICON_FULLSCREEN = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"></path></svg>`

const ICON_FULLSCREEN_EXIT = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3M3 16h3a2 2 0 0 1 2 2v3"></path></svg>`

// DOM 引用（用于动态更新控制按钮状态）
let fullscreenBtnEl: HTMLElement | null = null
let toolsBtnEl: HTMLElement | null = null

// ==================== 真全屏功能（Tauri 窗口级别） ====================

async function toggleTrueFullscreen() {
  const appWindow = getCurrentWindow()
  if (isTrueFullscreen.value) {
    // 退出全屏
    await appWindow.setFullscreen(false)
    isTrueFullscreen.value = false
    isFullscreen.value = false
    // 等待窗口调整大小完成
    await new Promise(r => setTimeout(r, 150))
    playerPosition.value = { ...savedPosition.value }
    playerSize.value = { ...savedSize.value }
  } else {
    // 保存当前位置和大小
    savedPosition.value = { ...playerPosition.value }
    savedSize.value = { ...playerSize.value }
    // 进入全屏
    await appWindow.setFullscreen(true)
    isTrueFullscreen.value = true
    isFullscreen.value = true
    // 等待窗口调整大小后填满视口
    await new Promise(r => setTimeout(r, 150))
    playerPosition.value = { x: 0, y: 0 }
    playerSize.value = { width: window.innerWidth, height: window.innerHeight }
  }
  // 更新全屏按钮图标
  if (fullscreenBtnEl) {
    fullscreenBtnEl.innerHTML = isTrueFullscreen.value ? ICON_FULLSCREEN_EXIT : ICON_FULLSCREEN
  }
}

function handleWindowResize() {
  if (isTrueFullscreen.value) {
    playerPosition.value = { x: 0, y: 0 }
    playerSize.value = { width: window.innerWidth, height: window.innerHeight }
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isTrueFullscreen.value) {
    e.preventDefault()
    toggleTrueFullscreen()
  }
}

// ==================== 构建 Artplayer 自定义控制按钮 ====================

function buildCustomControls(): Record<string, unknown>[] {
  const controls: Record<string, unknown>[] = []

  // 截图按钮（仅本地文件可用）
  if (isLocalFile.value) {
    controls.push({
      name: 'screenshot-btn',
      position: 'right',
      index: 10,
      html: ICON_SCREENSHOT,
      tooltip: '截图',
      style: { cursor: 'pointer' },
      click: () => handleScreenshot(),
    })
  }

  // 媒体工具按钮（仅本地文件可用）
  if (isLocalFile.value) {
    controls.push({
      name: 'tools-btn',
      position: 'right',
      index: 11,
      html: ICON_TOOLS,
      tooltip: '媒体工具',
      style: { cursor: 'pointer' },
      mounted: ($el: HTMLElement) => {
        toolsBtnEl = $el
      },
      click: () => {
        toggleToolsPanel()
        // 更新按钮高亮状态
        if (toolsBtnEl) {
          toolsBtnEl.style.color = showToolsPanel.value ? '#93a5f5' : ''
        }
      },
    })
  }

  // 下一个视频
  if (hasNextVideo.value) {
    controls.push({
      name: 'next-btn',
      position: 'left',
      index: 12,
      html: ICON_NEXT,
      tooltip: '下一个',
      style: { cursor: 'pointer' },
      click: () => handlePlayNext(),
    })
  }

  // 删除按钮
  controls.push({
    name: 'delete-btn',
    position: 'right',
    index: 13,
    html: ICON_DELETE,
    tooltip: '删除',
    style: { cursor: 'pointer' },
    click: () => handleDelete(),
  })

  // 真全屏按钮（替代 Artplayer 内置的 fullscreen 和 fullscreenWeb）
  controls.push({
    name: 'true-fullscreen',
    position: 'right',
    index: 100,
    html: ICON_FULLSCREEN,
    tooltip: '全屏',
    style: { cursor: 'pointer' },
    mounted: ($el: HTMLElement) => {
      fullscreenBtnEl = $el
    },
    click: () => toggleTrueFullscreen(),
  })

  return controls
}

// ==================== 原有播放器逻辑 ====================

const TRANSCODE_FORMATS = ['.mkv', '.avi', '.flv', '.wmv', '.rm', '.rmvb', '.ts', '.mpeg', '.mpg']

function resetPlaybackState() {
  error.value = null
  transcodingProgress.value = 0
}

function normalizeMediaPath(input: string): string {
  let value = input.trim()
  value = value.replace(/\\\//g, '/')
  value = value.replace(/^"(.*)"$/, '$1')
  return value
}

function isHttpUrl(path: string): boolean {
  return /^https?:\/\//i.test(path)
}

function needsTranscoding(url: string): boolean {
  const normalized = normalizeMediaPath(url)
  if (isHttpUrl(normalized)) {
    return false
  }
  const lowerUrl = normalized.toLowerCase()
  return TRANSCODE_FORMATS.some(ext => lowerUrl.endsWith(ext))
}

async function startVideoPlayback(filePath: string): Promise<{ url: string; isTranscoding: boolean }> {
  try {
    isTranscoding.value = true
    transcodingProgress.value = 0
    error.value = null
    transcodeMode.value = 'remux'

    const result = await invoke<[string, boolean]>('start_video_playback_cmd', {
      filePath,
      sessionId: props.videoId || Date.now().toString()
    })

    currentTranscodeSession.value = props.videoId || Date.now().toString()
    const [hlsUrl, needsTranscode] = result

    if (needsTranscode) {
      transcodeMode.value = 'transcode'
    } else {
      transcodeMode.value = 'remux'
    }

    return { url: hlsUrl, isTranscoding: needsTranscode }
  } catch (e) {
    error.value = '视频播放失败: ' + (e as Error).message
    throw e
  } finally {
    isTranscoding.value = false
  }
}

async function stopTranscoding() {
  if (currentTranscodeSession.value) {
    try {
      await invoke('stop_video_transcode', {
        sessionId: currentTranscodeSession.value
      })
    } catch (e) {
      console.error('[VideoPlayer] 停止转码失败:', e)
    }
    currentTranscodeSession.value = null
  }
}

async function openWithSystemPlayer() {
  try {
    const filePath = normalizeMediaPath(props.src)
    await invoke('open_with_system_player', { filePath })
    handleClose()
  } catch (e) {
    error.value = '打开系统播放器失败: ' + (e as Error).message
  }
}

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

async function getFileInfo(filePath: string): Promise<{ size: number; isLarge: boolean }> {
  try {
    const stats = await invoke<{ size: number }>('get_file_stats', { path: filePath })
    const sizeGB = stats.size / (1024 * 1024 * 1024)
    return { size: stats.size, isLarge: sizeGB > 1 }
  } catch (_e) {
    return { size: 0, isLarge: false }
  }
}

async function createPlayer() {
  if (!containerRef.value || !props.src) return

  resetPlaybackState()
  destroyPlayer()
  await new Promise(resolve => setTimeout(resolve, 100))

  const originalPath = normalizeMediaPath(props.src)
  let playUrl = originalPath
  let isHls = originalPath.toLowerCase().includes('.m3u8')

  if (needsTranscoding(originalPath)) {
    try {
      const result = await startVideoPlayback(originalPath)
      playUrl = result.url
      isHls = true

      if (result.isTranscoding) {
        const fileInfo = await getFileInfo(originalPath)
        if (fileInfo.isLarge) {
          transcodingProgress.value = 50
        }
      }
    } catch (_e) {
      error.value = '无法播放此视频格式，建议使用系统播放器'
      return
    }
  } else if (isHttpUrl(originalPath)) {
    playUrl = originalPath
  } else {
    playUrl = convertFileSrc(originalPath)
  }

  const customControls = buildCustomControls()
  
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
    fullscreen: false,
    fullscreenWeb: false,
    subtitleOffset: true,
    miniProgressBar: true,
    mutex: true,
    backdrop: true,
    playsInline: true,
    autoPlayback: true,
    lang: 'zh-cn',
    theme: '#1a1a2e',
    controls: customControls as any[],
    moreVideoAttr: {
      crossOrigin: 'anonymous'
    },
  }) as Artplayer
  
  art.title = props.title
  
  if (isHls && Hls.isSupported()) {
    const hls = new Hls({
      enableWorker: false,
      lowLatencyMode: false,
      backBufferLength: 90,
      maxBufferLength: 60,
      maxMaxBufferLength: 120,
      maxBufferSize: 50 * 1000 * 1000,
    })
    hlsRef.value = hls
    
    hls.loadSource(playUrl)
    hls.attachMedia(art.video)
    
    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      error.value = null
    })
    
    hls.on(Hls.Events.ERROR, (_event, data) => {
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
  
  art.on('ready', () => {
    error.value = null
  })

  art.video.addEventListener('playing', () => {
    error.value = null
  })
  
  art.on('error', () => {
    error.value = '播放出错，请重试'
  })
  
  artRef.value = art
}

function destroyPlayer() {
  // 退出真全屏
  if (isTrueFullscreen.value) {
    getCurrentWindow().setFullscreen(false)
    isTrueFullscreen.value = false
    isFullscreen.value = false
  }
  // 清理 DOM 引用
  fullscreenBtnEl = null
  toolsBtnEl = null
  if (hlsRef.value) {
    hlsRef.value.destroy()
    hlsRef.value = null
  }
  if (artRef.value) {
    artRef.value.destroy()
    artRef.value = null
  }
  stopTranscoding()
  resetPlaybackState()
}

async function handleClose() {
  if (isTrueFullscreen.value) {
    await getCurrentWindow().setFullscreen(false)
    isTrueFullscreen.value = false
    isFullscreen.value = false
    await new Promise(r => setTimeout(r, 100))
  }
  showToolsPanel.value = false
  destroyPlayer()
  emit('close')
}

function handlePlayNext() {
  if (hasNextVideo.value) {
    showToolsPanel.value = false
    resetPlaybackState()
    emit('playNext', props.currentIndex + 1)
  }
}

function handleDelete() {
  emit('deleteCurrent')
}

// 监听转换进度事件
async function setupConvertListener() {
  unlistenConvertProgress = await listen<ConvertTask>('convert-progress', (event) => {
    const update = event.payload
    // 匹配对应的任务
    if (convertTask.value && convertTask.value.id === update.id) {
      convertTask.value = update
      if (update.status === 'Completed' || update.status === 'Failed') {
        convertLoading.value = false
        if (update.status === 'Completed') {
          showToolMessage(`转换完成: ${update.output_path.split('/').pop()}`, 'success')
        }
      }
    }
    if (audioTask.value && audioTask.value.id === update.id) {
      audioTask.value = update
      if (update.status === 'Completed' || update.status === 'Failed') {
        audioLoading.value = false
        if (update.status === 'Completed') {
          showToolMessage(`音频提取完成: ${update.output_path.split('/').pop()}`, 'success')
        }
      }
    }
    if (gifTask.value && gifTask.value.id === update.id) {
      gifTask.value = update
      if (update.status === 'Completed' || update.status === 'Failed') {
        gifLoading.value = false
        if (update.status === 'Completed') {
          showToolMessage(`GIF导出完成: ${update.output_path.split('/').pop()}`, 'success')
        }
      }
    }
  })
}

// 生命周期
onMounted(() => {
  initPlayerPosition()
  setupConvertListener()
  window.addEventListener('resize', handleWindowResize)
  document.addEventListener('keydown', handleKeydown)
})

onBeforeUnmount(() => {
  window.removeEventListener('resize', handleWindowResize)
  document.removeEventListener('keydown', handleKeydown)
  destroyPlayer()
  if (unlistenConvertProgress) unlistenConvertProgress()
  if (toolMessageTimer) clearTimeout(toolMessageTimer)
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
      resetPlaybackState()
      initPlayerPosition()
      await nextTick()
      await new Promise(resolve => setTimeout(resolve, 100))
      await createPlayer()
    } finally {
      isCreating.value = false
    }
  } else {
    showToolsPanel.value = false
    destroyPlayer()
  }
})

// 监听源变化
watch(() => props.src, async (newSrc) => {
  if (props.visible && newSrc) {
    if (isCreating.value) return
    isCreating.value = true
    try {
      showToolsPanel.value = false
      resetPlaybackState()
      await nextTick()
      await new Promise(resolve => setTimeout(resolve, 100))
      await createPlayer()
    } finally {
      isCreating.value = false
    }
  }
})

// 格式化时间
function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60)
  const s = Math.floor(seconds % 60)
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
}

// 工具标签页配置
const toolTabs = [
  { key: 'convert' as const, label: '格式', icon: '📥' },
  { key: 'audio' as const, label: '音频', icon: '🎵' },
  { key: 'gif' as const, label: 'GIF', icon: '✂️' },
]
</script>

<template>
  <Transition name="fade">
    <div v-if="visible" class="fixed inset-0 z-[9999] pointer-events-none" @click.self="handleClose">
      <div
        class="absolute bg-bg-dark rounded-lg overflow-hidden shadow-player flex flex-col pointer-events-auto min-w-[400px] min-h-[225px] transition-all duration-200 group"
        :class="{ '!rounded-none': isFullscreen }"
        :style="{
          left: playerPosition.x + 'px',
          top: playerPosition.y + 'px',
          width: playerSize.width + 'px',
          height: playerSize.height + 'px'
        }"
      >
        <!-- ==================== 顶部栏：标题 + 关闭按钮 ==================== -->
        <div
          v-if="!isFullscreen"
          class="absolute top-0 inset-x-0 flex justify-between items-center px-4 py-3 bg-gradient-to-b from-[rgba(22,22,42,0.95)] to-transparent cursor-move select-none shrink-0 opacity-0 group-hover:opacity-100 transition-opacity duration-300 z-[100]"
          @mousedown="startDrag"
        >
          <span class="text-white text-sm font-medium truncate mr-4">{{ title }}</span>
            <button
            class="p-1.5 bg-transparent border-none text-white cursor-pointer rounded-md transition-all duration-200 flex items-center justify-center hover:bg-white/10 shrink-0"
            @click.stop="handleClose"
            title="关闭"
          >
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
        </div>

        <!-- ==================== 转码中提示 ==================== -->
        <div v-if="isTranscoding" class="absolute inset-0 flex items-center justify-center bg-[rgba(26,26,46,0.95)] z-50">
          <div class="text-center text-white">
            <div class="w-[50px] h-[50px] border-4 border-white/10 border-t-primary rounded-full animate-spin mx-auto mb-4"></div>
            <p v-if="transcodeMode === 'remux'" class="my-2 text-base">正在解复用视频...</p>
            <p v-else class="my-2 text-base">正在转码视频格式...</p>
            <p v-if="transcodeMode === 'remux'" class="text-xs text-text-muted">极速模式：不解码直接播放（约2-5秒）</p>
            <p v-else class="text-xs text-text-muted">首次播放需要转码，请稍候（约10-30秒）</p>
          </div>
        </div>

        <!-- ==================== 播放器容器 ==================== -->
        <div ref="containerRef" class="flex-1 min-h-0"></div>

        <!-- ==================== 错误遮罩 ==================== -->
        <div v-if="error && !isTranscoding" class="absolute inset-0 flex items-center justify-center bg-black/70 z-[60]">
          <div class="flex flex-col items-center gap-4 text-center p-5">
            <span class="text-red-400 text-sm">{{ error }}</span>
            <button
              class="px-5 py-2.5 gradient-primary text-white border-none rounded-lg text-sm font-medium cursor-pointer transition-all duration-200 hover:-translate-y-px hover:shadow-button"
              @click="openWithSystemPlayer"
            >
              使用系统播放器打开
            </button>
          </div>
        </div>

        <!-- ==================== 媒体工具面板（右侧） ==================== -->
        <Transition name="slide-right">
          <div
            v-if="showToolsPanel"
            class="absolute top-11 right-0 bottom-0 w-[220px] bg-[rgba(15,23,42,0.95)] backdrop-blur-xl border-l border-primary/20 z-[95] flex flex-col overflow-hidden"
            @click.stop
          >
            <!-- 工具标签页 -->
            <div class="flex px-2 pt-2 gap-0.5 border-b border-white/[0.06]">
              <button
                v-for="tab in toolTabs"
                :key="tab.key"
                class="flex-1 flex flex-col items-center gap-0.5 px-1 pt-2 pb-1.5 bg-transparent border-none cursor-pointer rounded-t-md transition-all duration-200 text-[10px]"
                :class="activeToolTab === tab.key
                  ? 'bg-primary/15 text-[#93a5f5]'
                  : 'text-text-muted hover:bg-white/5 hover:text-[#e2e8f0]'"
                @click="activeToolTab = tab.key"
              >
                <span class="text-base leading-none">{{ tab.icon }}</span>
                <span class="text-[10px] font-medium">{{ tab.label }}</span>
            </button>
            </div>

            <!-- 工具内容 -->
            <div class="flex-1 overflow-y-auto p-3">

              <!-- 格式转换 -->
              <div v-if="activeToolTab === 'convert'" class="flex flex-col gap-2.5">
                <div class="flex flex-col gap-1">
                  <label class="text-[11px] font-medium text-text-muted">输出格式</label>
                  <select v-model="convertFormat" class="tool-select">
                    <option value="mp4">MP4</option>
                    <option value="mkv">MKV</option>
                    <option value="webm">WebM</option>
                    <option value="mov">MOV</option>
                    <option value="avi">AVI</option>
                  </select>
                </div>
                <div v-if="convertTask && convertLoading" class="flex flex-col gap-1">
                  <div class="h-1 bg-white/10 rounded-full overflow-hidden">
                    <div class="h-full rounded-full bg-gradient-to-r from-primary to-primary-dark transition-[width] duration-300" :style="{ width: convertTask.progress + '%' }"></div>
                  </div>
                  <div class="flex justify-between text-[10px] text-text-muted">
                    <span>{{ convertTask.progress }}%</span>
                    <span class="max-w-[120px] overflow-hidden text-ellipsis whitespace-nowrap">{{ convertTask.message }}</span>
                  </div>
                </div>
                <div class="flex gap-1.5 mt-1">
                  <button
                    v-if="!convertLoading"
                    class="flex-1 py-[7px] px-3 border-none rounded-md text-xs font-medium cursor-pointer transition-all duration-200 text-center gradient-primary text-white hover:-translate-y-px hover:shadow-[0_3px_10px_rgba(102,126,234,0.3)]"
                    @click="handleConvert"
                  >开始转换</button>
                  <button
                    v-else
                    class="flex-1 py-[7px] px-3 border-none rounded-md text-xs font-medium cursor-pointer transition-all duration-200 text-center bg-gradient-to-br from-red-500 to-red-600 text-white hover:-translate-y-px hover:shadow-[0_3px_10px_rgba(239,68,68,0.3)]"
                    @click="handleStopConvert"
                  >停止转换</button>
          </div>
        </div>

              <!-- 音频提取 -->
              <div v-if="activeToolTab === 'audio'" class="flex flex-col gap-2.5">
                <div class="flex flex-col gap-1">
                  <label class="text-[11px] font-medium text-text-muted">音频格式</label>
                  <select v-model="audioFormat" class="tool-select">
                    <option value="mp3">MP3</option>
                    <option value="m4a">M4A</option>
                    <option value="wav">WAV</option>
                    <option value="flac">FLAC</option>
                    <option value="aac">AAC</option>
                  </select>
                </div>
                <div v-if="audioTask && audioLoading" class="flex flex-col gap-1">
                  <div class="h-1 bg-white/10 rounded-full overflow-hidden">
                    <div class="h-full rounded-full bg-gradient-to-r from-green-500 to-green-600 transition-[width] duration-300" :style="{ width: audioTask.progress + '%' }"></div>
                  </div>
                  <div class="flex justify-between text-[10px] text-text-muted">
                    <span>{{ audioTask.progress }}%</span>
                    <span class="max-w-[120px] overflow-hidden text-ellipsis whitespace-nowrap">{{ audioTask.message }}</span>
                  </div>
                </div>
                <div class="flex gap-1.5 mt-1">
                  <button
                    v-if="!audioLoading"
                    class="flex-1 py-[7px] px-3 border-none rounded-md text-xs font-medium cursor-pointer transition-all duration-200 text-center gradient-primary text-white hover:-translate-y-px hover:shadow-[0_3px_10px_rgba(102,126,234,0.3)]"
                    @click="handleAudioExtract"
                  >提取音频</button>
                  <button
                    v-else
                    class="flex-1 py-[7px] px-3 border-none rounded-md text-xs font-medium cursor-pointer transition-all duration-200 text-center bg-gradient-to-br from-red-500 to-red-600 text-white hover:-translate-y-px hover:shadow-[0_3px_10px_rgba(239,68,68,0.3)]"
                    @click="handleStopAudio"
                  >停止提取</button>
          </div>
        </div>

              <!-- GIF 导出 -->
              <div v-if="activeToolTab === 'gif'" class="flex flex-col gap-2.5">
                <div class="flex flex-col gap-1">
                  <label class="text-[11px] font-medium text-text-muted">开始 (秒)</label>
                  <input
                    type="number"
                    v-model.number="gifStartTime"
                    :min="0"
                    :max="getDuration()"
                    step="0.1"
                    class="tool-input"
                  />
          </div>
                <div class="flex flex-col gap-1">
                  <label class="text-[11px] font-medium text-text-muted">结束 (秒)</label>
                  <input
                    type="number"
                    v-model.number="gifEndTime"
                    :min="0"
                    :max="getDuration()"
                    step="0.1"
                    class="tool-input"
                  />
        </div>
                <div class="flex flex-col gap-1">
                  <label class="text-[11px] font-medium text-text-muted">帧率</label>
                  <select v-model.number="gifFps" class="tool-select">
                    <option :value="5">5 fps</option>
                    <option :value="10">10 fps</option>
                    <option :value="15">15 fps</option>
                    <option :value="20">20 fps</option>
                  </select>
                </div>
                <div class="flex flex-col gap-1">
                  <label class="text-[11px] font-medium text-text-muted">分辨率</label>
                  <select v-model="gifResolution" class="tool-select">
                    <option value="">原始</option>
                    <option value="640x360">640×360</option>
                    <option value="480x270">480×270</option>
                    <option value="320x180">320×180</option>
                  </select>
                </div>
                <div class="text-[10px] text-[#64748b] text-center py-0.5">
                  时间范围: {{ formatTime(gifStartTime) }} - {{ formatTime(gifEndTime) }}
                  ({{ (gifEndTime - gifStartTime).toFixed(1) }}s)
                </div>
                <div v-if="gifTask && gifLoading" class="flex flex-col gap-1">
                  <div class="h-1 bg-white/10 rounded-full overflow-hidden">
                    <div class="h-full rounded-full bg-gradient-to-r from-amber-500 to-amber-600 transition-[width] duration-300" :style="{ width: gifTask.progress + '%' }"></div>
                  </div>
                  <div class="flex justify-between text-[10px] text-text-muted">
                    <span>{{ gifTask.progress }}%</span>
                    <span class="max-w-[120px] overflow-hidden text-ellipsis whitespace-nowrap">{{ gifTask.message }}</span>
                  </div>
                </div>
                <div class="flex gap-1.5 mt-1">
                  <button
                    v-if="!gifLoading"
                    class="flex-1 py-[7px] px-3 border-none rounded-md text-xs font-medium cursor-pointer transition-all duration-200 text-center gradient-primary text-white hover:-translate-y-px hover:shadow-[0_3px_10px_rgba(102,126,234,0.3)]"
                    @click="handleGifExport"
                  >导出 GIF</button>
                  <button
                    v-else
                    class="flex-1 py-[7px] px-3 border-none rounded-md text-xs font-medium cursor-pointer transition-all duration-200 text-center bg-gradient-to-br from-red-500 to-red-600 text-white hover:-translate-y-px hover:shadow-[0_3px_10px_rgba(239,68,68,0.3)]"
                    @click="handleStopGif"
                  >停止导出</button>
                </div>
              </div>
            </div>
          </div>
        </Transition>

        <!-- ==================== 工具操作提示 Toast ==================== -->
        <Transition name="toast">
          <div
            v-if="toolMessage"
            class="absolute bottom-[60px] left-1/2 -translate-x-1/2 px-4 py-2 rounded-lg text-xs font-medium z-[200] whitespace-nowrap max-w-[80%] overflow-hidden text-ellipsis pointer-events-none"
            :class="{
              'bg-green-500/90 text-white': toolMessage.type === 'success',
              'bg-red-500/90 text-white': toolMessage.type === 'error',
              'bg-blue-500/90 text-white': toolMessage.type === 'info',
            }"
          >
            {{ toolMessage.text }}
          </div>
        </Transition>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* ==================== 工具面板表单控件 ==================== */
.tool-select,
.tool-input {
  padding: 6px 8px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  color: #e2e8f0;
  font-size: 12px;
  outline: none;
  transition: border-color 0.2s;
  width: 100%;
  box-sizing: border-box;
}

.tool-select:focus,
.tool-input:focus {
  border-color: rgba(102, 126, 234, 0.5);
}

.tool-select option {
  background: #1e293b;
  color: #e2e8f0;
}

/* ==================== Vue Transition 动画 ==================== */
.slide-right-enter-active,
.slide-right-leave-active {
  transition: transform 0.25s ease, opacity 0.25s ease;
}

.slide-right-enter-from,
.slide-right-leave-to {
  transform: translateX(100%);
  opacity: 0;
}

.toast-enter-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.toast-leave-active {
  transition: opacity 0.3s ease, transform 0.3s ease;
}

.toast-enter-from {
  opacity: 0;
  transform: translateX(-50%) translateY(10px);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-10px);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* ==================== Artplayer 深度样式 ==================== */
:deep(.art-progress-played) {
  background: #667eea !important;
}

:deep(.art-time) {
  color: white !important;
}

/* 自定义控制按钮样式 */
:deep(.art-control-screenshot-btn),
:deep(.art-control-tools-btn),
:deep(.art-control-next-btn),
:deep(.art-control-delete-btn),
:deep(.art-control-true-fullscreen) {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 6px;
  transition: color 0.2s, opacity 0.2s;
  opacity: 0.85;
}

:deep(.art-control-screenshot-btn:hover),
:deep(.art-control-tools-btn:hover),
:deep(.art-control-next-btn:hover),
:deep(.art-control-true-fullscreen:hover) {
  opacity: 1;
  color: #93a5f5;
}

:deep(.art-control-delete-btn:hover) {
  opacity: 1;
  color: #fca5a5;
}
</style>
