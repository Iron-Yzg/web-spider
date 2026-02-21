<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { YtdlpTask, YtdlpTaskStatus, LocalVideo } from '../types'
import {
  getConfig,
  getYtdlpTasks,
  startYtdlpTask,
  stopYtdlpTask,
  deleteYtdlpTask,
  cleanupYtdlpTasks,
  openPath,
} from '../services/api'
import AddTaskDialog from '../components/AddTaskDialog.vue'
import VideoPlayer from '../components/VideoPlayer.vue'
import DlnaCastDialog from '../components/DlnaCastDialog.vue'
import IconButton from '../components/IconButton.vue'

// 任务列表
const tasks = ref<YtdlpTask[]>([])

// yt-dlp 状态
const ytdlpAvailable = ref(true)

// 下载配置
const downloadPath = ref('./downloads')

// 搜索和筛选
const searchQuery = ref('')
const statusFilter = ref<YtdlpTaskStatus | ''>('')

// 弹窗状态
const showAddDialog = ref(false)

// DLNA 投屏弹窗
const showDlnaDialog = ref(false)
const dlnaVideo = ref<LocalVideo | null>(null)

function openDlnaDialog(task: YtdlpTask) {
  if (!task.file_path) return
  dlnaVideo.value = {
    id: task.id,
    name: task.title,
    file_path: task.file_path,
    file_size: '',
    duration: '',
    resolution: task.resolution || '',
    added_at: '',
  }
  showDlnaDialog.value = true
}

function closeDlnaDialog() {
  showDlnaDialog.value = false
  dlnaVideo.value = null
}

// 监听器
let unlistenProgress: (() => void) | null = null
let unlistenComplete: (() => void) | null = null

onMounted(async () => {

  // 加载下载配置
  try {
    const config = await getConfig()
    downloadPath.value = config.download_path || './downloads'
  } catch (e) {
    console.error('加载配置失败:', e)
  }

  // 加载任务列表
  await refreshTasks()

  // 监听进度
  unlistenProgress = await listen<YtdlpTask>('ytdlp-progress', async (event: { payload: YtdlpTask }) => {
    const task = event.payload
    const index = tasks.value.findIndex(t => t.id === task.id)
    if (index !== -1) {
      tasks.value[index] = task
    } else {
      tasks.value.push(task)
    }
  })

  // 监听下载完成
  unlistenComplete = await listen<YtdlpTask>('ytdlp-complete', async (event: { payload: YtdlpTask }) => {
    const completedTask = event.payload
    console.log('[ytdlp-complete] 收到完成事件:', completedTask.id, 'status:', completedTask.status)
    const index = tasks.value.findIndex(t => t.id === completedTask.id)
    if (index !== -1) {
      tasks.value[index] = completedTask
    }
  })
})

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress()
  if (unlistenComplete) unlistenComplete()
})

// 刷新任务列表
async function refreshTasks() {
  try {
    const result = await getYtdlpTasks()
    tasks.value = result
  } catch (e) {
    console.error('刷新任务列表失败:', e)
  }
}

// 判断任务是否可以开始
function canStart(task: YtdlpTask): boolean {
  return task.status !== 'Completed' && task.status !== 'Downloading'
}

// 打开添加弹窗
function openAddDialog() {
  showAddDialog.value = true
}

// 关闭添加弹窗
function closeAddDialog() {
  showAddDialog.value = false
}

// 处理添加任务（从弹窗接收任务列表）
async function handleAddTasks(_tasks: YtdlpTask[]) {
  // 弹窗已经调用了 addYtdlpTasks，所以这里只需要刷新列表
  closeAddDialog()
  await refreshTasks()
}

// 停止任务
async function stopTask(taskId: string) {
  try {
    await stopYtdlpTask(taskId)
    await refreshTasks()
  } catch (e) {
    console.error('停止任务失败:', e)
  }
}

// 开始任务（断点续传）
async function startTask(taskId: string) {
  try {
    await startYtdlpTask(taskId, downloadPath.value)
  } catch (e) {
    console.error('开始任务失败:', e)
    await refreshTasks()
  }
}

// 删除任务（先停止下载，再删除）
async function deleteTask(taskId: string) {
  try {
    // 如果任务正在运行，先停止
    const task = tasks.value.find(t => t.id === taskId)
    if (task && task.status === 'Downloading') {
      await stopYtdlpTask(taskId)
    }
    // 从数据库删除任务（会清理临时文件）
    await deleteYtdlpTask(taskId)
    tasks.value = tasks.value.filter(t => t.id !== taskId)
  } catch (e) {
    console.error('删除任务失败:', e)
  }
}

// 清理已完成的任务
async function cleanupTasks() {
  try {
    await cleanupYtdlpTasks()
    await refreshTasks()
  } catch (e) {
    console.error('清理任务失败:', e)
  }
}

// 获取状态文本
function getStatusText(status: YtdlpTaskStatus): string {
  const map: Record<string, string> = {
    'Pending': '等待中',
    'Queued': '已队列',
    'Downloading': '下载中',
    'Paused': '已暂停',
    'Completed': '已完成',
    'Failed': '失败',
    'Cancelled': '已取消',
  }
  return map[status] || status
}

// 获取状态样式类
function getStatusClass(status: YtdlpTaskStatus): string {
  const map: Record<string, string> = {
    'Pending': 'bg-amber-100 text-amber-700',
    'Queued': 'bg-blue-100 text-blue-700',
    'Downloading': 'bg-green-100 text-green-700',
    'Paused': 'bg-amber-100 text-amber-700',
    'Completed': 'bg-green-100 text-green-700',
    'Failed': 'bg-red-100 text-red-700',
    'Cancelled': 'bg-gray-100 text-gray-500',
  }
  return map[status] || ''
}

// 打开文件所在文件夹
async function openFolder(filePath: string) {
  if (!filePath) {
    console.error('文件路径为空')
    return
  }
  try {
    // 如果是文件路径，提取目录
    const path = filePath.includes('/') || filePath.includes('\\')
      ? filePath.substring(0, filePath.lastIndexOf('/'))
      : filePath
    await openPath(path)
  } catch (e) {
    console.error('打开文件夹失败:', e)
  }
}

// 筛选任务
const filteredTasks = ref<YtdlpTask[]>([])

// 播放器状态
const playerVisible = ref(false)
const playerSrc = ref('')
const playerTitle = ref('')
const playerFilePath = ref('')
const playerPlaylist = ref<YtdlpTask[]>([])
const currentVideoIndex = ref(0)
const videoRefs = ref<Record<string, HTMLVideoElement>>({})

// 视频加载完成后跳转到第一帧并暂停
function handleVideoLoaded(videoId: string) {
  const video = videoRefs.value[videoId]
  if (video) {
    video.currentTime = 0.1  // 跳转到 0.1 秒（确保不是黑屏）
    video.pause()
  }
}

// 打开播放器播放本地视频
async function openPlayer(task: YtdlpTask) {
  if (task.file_path) {
    // 传递原始文件路径，由 VideoPlayer 内部处理路径转换
    playerSrc.value = task.file_path
    playerTitle.value = task.title || '本地视频'
    playerFilePath.value = task.file_path
    playerPlaylist.value = filteredTasks.value.filter(t => t.file_path)
    currentVideoIndex.value = filteredTasks.value.findIndex(t => t.id === task.id)
    playerVisible.value = true
  }
}

// 处理播放下一个视频
function handlePlayNext(nextIndex: number) {
  if (nextIndex >= 0 && nextIndex < playerPlaylist.value.length) {
    const nextTask = playerPlaylist.value[nextIndex]
    if (nextTask.file_path) {
      // 传递原始文件路径，由 VideoPlayer 内部处理路径转换
      playerSrc.value = nextTask.file_path
      playerTitle.value = nextTask.title || '本地视频'
      playerFilePath.value = nextTask.file_path
      currentVideoIndex.value = nextIndex
    }
  }
}

// 处理删除当前视频
async function handleDeleteCurrent() {
  const currentTask = playerPlaylist.value[currentVideoIndex.value]
  if (!currentTask) return

  try {
    await deleteYtdlpTask(currentTask.id)
    // 从播放列表中移除
    playerPlaylist.value.splice(currentVideoIndex.value, 1)

    // 如果还有视频，播放下一个
    if (playerPlaylist.value.length > 0) {
      const nextIndex = currentVideoIndex.value >= playerPlaylist.value.length
        ? playerPlaylist.value.length - 1
        : currentVideoIndex.value
      const nextTask = playerPlaylist.value[nextIndex]
      if (nextTask.file_path) {
        // 传递原始文件路径，由 VideoPlayer 内部处理路径转换
        playerSrc.value = nextTask.file_path
        playerTitle.value = nextTask.title || '本地视频'
        playerFilePath.value = nextTask.file_path
        currentVideoIndex.value = nextIndex
      }
    } else {
      // 没有视频了，关闭播放器
      handlePlayerClose()
    }
  } catch (e) {
    console.error('删除视频失败:', e)
  }
}

// 关闭播放器
function handlePlayerClose() {
  playerVisible.value = false
  playerSrc.value = ''
  playerTitle.value = ''
  playerFilePath.value = ''
  playerPlaylist.value = []
  currentVideoIndex.value = 0
}

watch([searchQuery, statusFilter, () => tasks.value], () => {
  let result = tasks.value

  // 按状态筛选
  if (statusFilter.value) {
    result = result.filter(t => t.status === statusFilter.value)
  }

  // 按标题搜索
  if (searchQuery.value.trim()) {
    const query = searchQuery.value.toLowerCase()
    result = result.filter(t =>
      t.title.toLowerCase().includes(query) ||
      t.url.toLowerCase().includes(query)
    )
  }

  filteredTasks.value = result
}, { immediate: true, deep: true })
</script>

<template>
  <div class="h-full flex flex-col bg-white rounded-xl shadow-[0_2px_12px_rgba(0,0,0,0.06)] overflow-hidden">
    <div class="flex-1 flex flex-col overflow-hidden">
      <div class="flex justify-between items-center px-5 py-3 bg-[#fafbfc] border-b border-[#f0f0f0] shrink-0">
        <div class="flex items-center gap-3"><span class="text-sm font-semibold text-[#1a1a2e]">下载任务 ({{ filteredTasks.length }}/{{ tasks.length }})</span></div>
        <div class="flex items-center gap-2.5">
          <input type="text" v-model="searchQuery" placeholder="搜索任务名称" class="px-3 py-1.5 border border-[#e8e8e8] rounded-md text-[13px] w-[180px] transition-all focus:outline-none focus:border-[#667eea]" />
          <select v-model="statusFilter" class="px-3 py-1.5 border border-[#e8e8e8] rounded-md text-[13px] bg-white cursor-pointer transition-all focus:outline-none focus:border-[#667eea]">
            <option value="">全部状态</option><option value="Pending">等待中</option><option value="Queued">已队列</option><option value="Downloading">下载中</option><option value="Paused">已暂停</option><option value="Completed">已完成</option><option value="Failed">失败</option><option value="Cancelled">已取消</option>
          </select>
          <button v-if="tasks.some(t => ['Completed', 'Failed', 'Cancelled'].includes(t.status))" @click="cleanupTasks" class="px-3 py-1 bg-transparent text-[#667eea] border border-[#667eea] rounded-md text-xs cursor-pointer transition-all hover:bg-[#667eea] hover:text-white">清理已完成</button>
          <button class="inline-flex items-center gap-1 px-3 py-1.5 border-none rounded-md text-xs cursor-pointer transition-all text-white bg-[linear-gradient(135deg,#6366f1_0%,#8b5cf6_100%)] hover:-translate-y-0.5 hover:shadow-[0_4px_12px_rgba(99,102,241,0.4)] disabled:opacity-50 disabled:cursor-not-allowed" @click="openAddDialog" :disabled="!ytdlpAvailable" title="添加下载"><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>添加</button>
        </div>
      </div>

      <div v-if="tasks.length === 0" class="flex-1 flex flex-col items-center justify-center p-[60px] text-center">
        <div class="text-[#334155] mb-4"><svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg></div>
        <p class="text-base text-[#64748b] mb-5">暂无下载任务</p>
        <button @click="openAddDialog" class="px-6 py-2.5 border-none rounded-lg text-sm text-white cursor-pointer bg-[linear-gradient(135deg,#6366f1_0%,#8b5cf6_100%)] disabled:opacity-50 disabled:cursor-not-allowed" :disabled="!ytdlpAvailable">添加下载</button>
      </div>

      <div v-else class="flex-1 flex flex-col overflow-hidden">
        <div class="flex px-5 py-2.5 bg-[#f8f9fa] border-b border-[#eee] text-xs font-semibold text-[#64748b] uppercase tracking-[0.5px] items-center">
          <span class="w-[60px] mr-3">封面</span><span class="flex-1 min-w-0 pr-4">名称</span><span class="w-[120px] pr-4">状态</span><span class="w-[120px] shrink-0">操作</span>
        </div>

        <div class="flex-1 overflow-y-auto">
          <div v-if="filteredTasks.length === 0" class="py-10 px-5 text-center text-[#94a3b8] text-[13px]">没有找到匹配的任务</div>

          <div v-for="task in filteredTasks" :key="task.id" class="flex items-center px-5 py-3 border-b border-[#f5f5f5] transition-colors hover:bg-[#fafbfc]" :class="{ 'bg-[#f0f9ff]': task.status === 'Downloading' }">
            <div class="w-[60px] h-[34px] mr-3 relative overflow-hidden rounded bg-[#f5f5f5] shrink-0">
              <video v-if="task.status === 'Completed' && task.file_path" :ref="el => videoRefs[task.id] = el as HTMLVideoElement" :src="convertFileSrc(task.file_path)" class="w-full h-full object-cover bg-black" muted preload="auto" @loadeddata="handleVideoLoaded(task.id)"></video>
              <div v-else class="w-full h-full flex items-center justify-center bg-[#f8f9fa] text-[#cbd5e1]"><svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"></rect><line x1="7" y1="2" x2="7" y2="22"></line><line x1="17" y1="2" x2="17" y2="22"></line><line x1="2" y1="12" x2="22" y2="12"></line></svg></div>
            </div>

            <div class="flex-1 min-w-0 pr-4">
              <span class="block text-sm font-medium text-[#1a1a2e] whitespace-nowrap overflow-hidden text-ellipsis" :title="task.title">{{ task.title || '未知标题' }}</span>
              <span class="block mt-0.5 text-[11px] text-[#94a3b8] font-mono whitespace-nowrap overflow-hidden text-ellipsis" :title="task.url">{{ task.url }}</span>
              <div v-if="task.status === 'Downloading'" class="mt-1 flex gap-3 text-[11px]"><span class="text-[#16a34a] font-semibold">{{ Math.round(task.progress) }}%</span><span v-if="task.speed" class="text-[#64748b]">{{ task.speed }}</span></div>
              <div v-if="task.status === 'Completed' && task.file_path" class="mt-1"><span class="text-[11px] text-[#64748b] font-mono bg-[#f8f9fa] px-1.5 py-0.5 rounded">{{ task.file_path }}</span></div>
            </div>

            <div class="w-[120px] pr-4">
              <div v-if="task.status === 'Downloading'" class="w-full"><div class="h-1.5 bg-[#e5e7eb] rounded-[3px] overflow-hidden"><div class="h-full rounded-[3px] bg-[linear-gradient(90deg,#22c55e,#16a34a)] transition-all duration-300" :style="{ width: task.progress + '%' }"></div></div></div>
              <div v-else-if="task.status === 'Failed'" class="relative inline-block group"><span :class="['inline-block px-2.5 py-1 rounded-full text-[11px] font-medium', getStatusClass(task.status)]">{{ getStatusText(task.status) }}</span><span class="invisible group-hover:visible absolute bottom-full left-1/2 -translate-x-1/2 bg-[#1e293b] text-[#fee2e2] px-3 py-2 rounded text-[11px] whitespace-nowrap max-w-[300px] overflow-hidden text-ellipsis z-[100] mb-1 shadow-[0_4px_12px_rgba(0,0,0,0.3)]">{{ task.message || '下载失败' }}</span></div>
              <span v-else :class="['inline-block px-2.5 py-1 rounded-full text-[11px] font-medium', getStatusClass(task.status)]">{{ getStatusText(task.status) }}</span>
            </div>

            <div class="w-[120px] flex items-center gap-1.5 shrink-0">
              <IconButton v-if="task.status === 'Downloading'" variant="stop" title="停止" @click="stopTask(task.id)" />
              <IconButton v-else-if="canStart(task)" variant="start" title="开始" @click="startTask(task.id)" />
              <template v-else-if="task.status === 'Completed' && task.file_path">
                <IconButton variant="play" title="播放" @click="openPlayer(task)" />
                <IconButton variant="folder" title="打开文件夹" @click="openFolder(task.file_path!)" />
                <IconButton variant="cast" title="投屏" @click="openDlnaDialog(task)" />
              </template>
              <IconButton v-else variant="delete" title="删除" @click="deleteTask(task.id)" />
            </div>
          </div>
        </div>
      </div>
    </div>

    <AddTaskDialog :visible="showAddDialog" @close="closeAddDialog" @confirm="handleAddTasks" />
    <VideoPlayer v-show="playerVisible" :visible="playerVisible" :src="playerSrc" :title="playerTitle" :playlist="playerPlaylist" :current-index="currentVideoIndex" @close="handlePlayerClose" @play-next="handlePlayNext" @delete-current="handleDeleteCurrent" />
    <DlnaCastDialog v-if="dlnaVideo" :video="dlnaVideo" @close="closeDlnaDialog" />
  </div>
</template>

<style scoped>
</style>
