<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { YtdlpTask, YtdlpTaskStatus } from '../types'
import VideoPlayer from './VideoPlayer.vue'

// 任务列表
const tasks = ref<YtdlpTask[]>([])

// 正在运行的任务ID集合
const runningTaskIds = ref<Set<string>>(new Set())

// yt-dlp 状态
const ytdlpAvailable = ref(false)
const ytdlpVersion = ref('')

// 下载配置
const downloadPath = ref('./downloads')

// 搜索和筛选
const searchQuery = ref('')
const statusFilter = ref<YtdlpTaskStatus | ''>('')

// 弹窗状态
const showAddDialog = ref(false)
const urlInput = ref('')
const urls = ref<string[]>([])
const errorMessage = ref('')
const successMessage = ref('')

// 监听器
let unlistenProgress: (() => void) | null = null
let unlistenComplete: (() => void) | null = null

onMounted(async () => {
  // 检查 yt-dlp
  try {
    ytdlpAvailable.value = await invoke<boolean>('check_ytdlp')
    if (ytdlpAvailable.value) {
      ytdlpVersion.value = await invoke<string>('get_ytdlp_version')
    }
  } catch (e) {
    console.error('检查 yt-dlp 失败:', e)
  }

  // 加载下载配置
  try {
    const config = await invoke<any>('get_config')
    downloadPath.value = config.download_path || './downloads'
  } catch (e) {
    console.error('加载配置失败:', e)
  }

  // 加载任务列表
  await refreshTasks()

  // 监听进度
  unlistenProgress = await listen<YtdlpTask>('ytdlp-progress', async (event: { payload: YtdlpTask }) => {
    const task = event.payload
    // 标记为运行中
    runningTaskIds.value.add(task.id)

    const index = tasks.value.findIndex(t => t.id === task.id)
    if (index !== -1) {
      // 更新现有任务
      tasks.value[index] = task
    } else {
      tasks.value.push(task)
    }
  })

  // 监听下载完成
  unlistenComplete = await listen<YtdlpTask>('ytdlp-complete', async (event: { payload: YtdlpTask }) => {
    const completedTask = event.payload
    // 从运行中移除
    runningTaskIds.value.delete(completedTask.id)

    // 从数据库刷新任务列表，确保状态最新
    await refreshTasks()
  })
})

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress()
  if (unlistenComplete) unlistenComplete()
})

// 刷新任务列表
async function refreshTasks() {
  try {
    const result = await invoke<YtdlpTask[]>('get_ytdlp_tasks')
    // 调试：检查第一个任务的thumbnail
    if (result.length > 0) {
      console.log('First task thumbnail:', result[0].thumbnail)
      console.log('First task title:', result[0].title)
    }
    tasks.value = result
  } catch (e) {
    console.error('刷新任务列表失败:', e)
  }
}

// 判断任务是否正在运行
function isTaskRunning(taskId: string): boolean {
  return runningTaskIds.value.has(taskId)
}

// 判断任务是否可以开始（已暂停/失败/取消 且 没有在运行）- 已完成不算
function canStart(task: YtdlpTask): boolean {
  return task.status !== 'Completed' && !isTaskRunning(task.id)
}

// 判断任务是否可以删除（未在运行中）
function canDelete(task: YtdlpTask): boolean {
  return !isTaskRunning(task.id)
}

// 打开添加弹窗
function openAddDialog() {
  showAddDialog.value = true
  urlInput.value = ''
  urls.value = []
  errorMessage.value = ''
  successMessage.value = ''
}

// 关闭添加弹窗
function closeAddDialog() {
  showAddDialog.value = false
}

// 移除 URL
function removeUrl(index: number) {
  urls.value.splice(index, 1)
}

// 验证 URL
function isValidUrl(url: string): boolean {
  try {
    new URL(url)
    return true
  } catch {
    return false
  }
}

// 批量解析 URL
function parseMultipleUrls(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text') || ''
  const lines = text.split(/[\n,]/).map(u => u.trim()).filter(u => u)
  for (const url of lines) {
    if (isValidUrl(url) && !urls.value.includes(url)) {
      urls.value.push(url)
    }
  }
}

// 添加任务（只添加，不启动）
async function addTasks() {
  if (urls.value.length === 0) {
    errorMessage.value = '请输入视频链接'
    return
  }

  errorMessage.value = ''
  successMessage.value = ''

  try {
    await invoke('add_ytdlp_tasks', {
      urls: urls.value,
      outputPath: downloadPath.value,
    })

    successMessage.value = `已添加 ${urls.value.length} 个任务`
  } catch (e) {
    errorMessage.value = '添加任务失败: ' + e
  } finally {
    closeAddDialog()
    await refreshTasks()
  }
}

// 停止任务
async function stopTask(taskId: string) {
  try {
    await invoke('stop_ytdlp_task', { taskId: taskId })
    runningTaskIds.value.delete(taskId)
    await refreshTasks()
  } catch (e) {
    console.error('停止任务失败:', e)
  }
}

// 开始任务（断点续传）
async function startTask(taskId: string) {
  try {
    await invoke('start_ytdlp_task', {
      taskId: taskId,
      outputPath: downloadPath.value,
    })
    runningTaskIds.value.add(taskId)
  } catch (e) {
    console.error('开始任务失败:', e)
    runningTaskIds.value.delete(taskId)
    await refreshTasks()
  }
}

// 删除任务（先停止下载，再删除）
async function deleteTask(taskId: string) {
  try {
    // 如果任务正在运行，先停止
    if (isTaskRunning(taskId)) {
      await invoke('stop_ytdlp_task', { taskId: taskId })
      runningTaskIds.value.delete(taskId)
    }
    // 从数据库删除任务（会清理临时文件）
    await invoke('delete_ytdlp_task', { taskId: taskId })
    tasks.value = tasks.value.filter(t => t.id !== taskId)
  } catch (e) {
    console.error('删除任务失败:', e)
  }
}

// 清理已完成的任务
async function cleanupTasks() {
  try {
    await invoke('cleanup_ytdlp_tasks')
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
    'Pending': 'status-pending',
    'Queued': 'status-queued',
    'Downloading': 'status-downloading',
    'Paused': 'status-paused',
    'Completed': 'status-completed',
    'Failed': 'status-failed',
    'Cancelled': 'status-cancelled',
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
    await invoke('open_path', { path })
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

// 打开播放器播放本地视频
async function openPlayer(task: YtdlpTask) {
  if (task.file_path) {
    // 转换本地路径为 asset:// URL
    const decodedPath = decodeURIComponent(task.file_path)
    const assetUrl = convertFileSrc(decodedPath)
    playerSrc.value = assetUrl
    playerTitle.value = task.title || '本地视频'
    playerFilePath.value = task.file_path
    playerVisible.value = true
  }
}

// 关闭播放器
function handlePlayerClose() {
  playerVisible.value = false
  playerSrc.value = ''
  playerTitle.value = ''
  playerFilePath.value = ''
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
  <div class="download-page">
    <!-- 任务列表 -->
    <div class="task-section">
      <div class="section-header">
        <div class="header-left">
          <span class="section-title">下载任务 ({{ filteredTasks.length }}/{{ tasks.length }})</span>
        </div>
        <div class="header-right">
          <!-- 搜索框 -->
          <input
            type="text"
            v-model="searchQuery"
            placeholder="搜索任务名称"
            class="filter-input"
          />
          <!-- 状态筛选 -->
          <select v-model="statusFilter" class="filter-select">
            <option value="">全部状态</option>
            <option value="Pending">等待中</option>
            <option value="Queued">已队列</option>
            <option value="Downloading">下载中</option>
            <option value="Paused">已暂停</option>
            <option value="Completed">已完成</option>
            <option value="Failed">失败</option>
            <option value="Cancelled">已取消</option>
          </select>
          <button
            v-if="tasks.some(t => ['Completed', 'Failed', 'Cancelled'].includes(t.status))"
            @click="cleanupTasks"
            class="cleanup-btn"
          >
            清理已完成
          </button>
          <button
            class="add-btn-small"
            @click="openAddDialog"
            :disabled="!ytdlpAvailable"
            title="添加下载"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="12" y1="5" x2="12" y2="19"></line>
              <line x1="5" y1="12" x2="19" y2="12"></line>
            </svg>
            添加
          </button>
        </div>
      </div>

      <!-- 空状态 -->
      <div v-if="tasks.length === 0" class="empty-state">
        <div class="empty-icon">
          <svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
            <polyline points="7 10 12 15 17 10"></polyline>
            <line x1="12" y1="15" x2="12" y2="3"></line>
          </svg>
        </div>
        <p class="empty-text">暂无下载任务</p>
        <button @click="openAddDialog" class="go-add-btn" :disabled="!ytdlpAvailable">添加下载</button>
      </div>

      <!-- 任务表格 -->
      <div v-else class="task-table">
        <div class="table-header">
          <span class="col-name">封面</span>
          <span class="col-name">名称</span>
          <span class="col-status">状态</span>
          <span class="col-action">操作</span>
        </div>

        <div class="table-body">
          <div v-if="filteredTasks.length === 0" class="empty-tip">
            没有找到匹配的任务
          </div>

          <div v-for="task in filteredTasks" :key="task.id" class="table-row" :class="{ running: isTaskRunning(task.id) }">
            <!-- 封面 -->
            <div class="col-cover">
              <img
                v-if="task.thumbnail"
                :src="task.thumbnail"
                :alt="task.title"
                class="cover-thumbnail"
              />
              <div v-else class="cover-placeholder-small">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"></rect>
                  <line x1="7" y1="2" x2="7" y2="22"></line>
                  <line x1="17" y1="2" x2="17" y2="22"></line>
                  <line x1="2" y1="12" x2="22" y2="12"></line>
                </svg>
              </div>
            </div>

            <!-- 名称 -->
            <div class="col-name">
              <span class="task-title" :title="task.title">{{ task.title || '未知标题' }}</span>
              <span class="task-url" :title="task.url">{{ task.url }}</span>
              <!-- 下载中显示进度信息 -->
              <div v-if="task.status === 'Downloading' && isTaskRunning(task.id)" class="task-progress-info">
                <span class="progress-percent">{{ Math.round(task.progress) }}%</span>
                <span v-if="task.speed" class="progress-speed">{{ task.speed }}</span>
              </div>
              <!-- 已完成显示文件路径 -->
              <div v-if="task.status === 'Completed' && task.file_path" class="task-file-info">
                <span class="file-path">{{ task.file_path }}</span>
              </div>
            </div>

            <!-- 状态 -->
            <div class="col-status">
              <!-- 下载中显示进度条 -->
              <div v-if="task.status === 'Downloading' && isTaskRunning(task.id)" class="task-progress">
                <div class="progress-bar">
                  <div class="progress-fill" :style="{ width: task.progress + '%' }"></div>
                </div>
              </div>
              <!-- 失败状态显示错误提示 -->
              <div v-else-if="task.status === 'Failed'" class="status-error">
                <span class="status-tag status-failed">{{ getStatusText(task.status) }}</span>
                <span class="error-tooltip">{{ task.message || '下载失败' }}</span>
              </div>
              <!-- 其他状态显示标签 -->
              <span v-else :class="['status-tag', getStatusClass(task.status)]">
                {{ getStatusText(task.status) }}
              </span>
            </div>

            <!-- 操作 -->
            <div class="col-action">
              <!-- 下载中显示停止 -->
              <button
                v-if="isTaskRunning(task.id)"
                @click="stopTask(task.id)"
                class="action-btn stop"
                title="停止"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                  <rect x="6" y="4" width="4" height="16"></rect>
                  <rect x="14" y="4" width="4" height="16"></rect>
                </svg>
              </button>

              <!-- 已暂停/失败/取消（未运行）显示开始 -->
              <button
                v-else-if="canStart(task)"
                @click="startTask(task.id)"
                class="action-btn start"
                title="开始"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                  <polygon points="5 3 19 12 5 21 5 3"></polygon>
                </svg>
              </button>

              <!-- 已完成（未运行）显示播放和文件夹 -->
              <template v-else-if="task.status === 'Completed' && task.file_path">
                <button @click="openPlayer(task)" class="action-btn play" title="播放">
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polygon points="5 3 19 12 5 21 5 3"></polygon>
                  </svg>
                </button>
                <button @click="openFolder(task.file_path!)" class="action-btn folder" title="打开文件夹">
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
                  </svg>
                </button>
              </template>

              <!-- 未运行且不是已完成的任务显示删除 -->
              <button
                v-if="canDelete(task) && task.status !== 'Completed'"
                @click="deleteTask(task.id)"
                class="action-btn delete"
                title="删除"
              >
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

    <!-- 添加下载弹窗 -->
    <div v-if="showAddDialog" class="dialog-overlay" @click.self="closeAddDialog">
      <div class="dialog">
        <div class="dialog-header">
          <h3>添加下载</h3>
          <button @click="closeAddDialog" class="close-btn">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        </div>

        <div class="dialog-body">
          <div class="form-group">
            <label>下载路径</label>
            <input type="text" v-model="downloadPath" class="form-input" placeholder="输入下载路径" />
          </div>

          <div class="form-group">
            <label>视频链接</label>
            <textarea v-model="urlInput" @paste="parseMultipleUrls" class="form-textarea" placeholder="输入视频链接（支持粘贴多个，换行或逗号分隔）"></textarea>
          </div>

          <!-- 已添加的链接列表 -->
          <div v-if="urls.length > 0" class="url-list">
            <div v-for="(url, index) in urls" :key="index" class="url-item">
              <span class="url-text">{{ url }}</span>
              <button @click="removeUrl(index)" class="remove-url">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="18" y1="6" x2="6" y2="18"></line>
                  <line x1="6" y1="6" x2="18" y2="18"></line>
                </svg>
              </button>
            </div>
          </div>

          <div v-if="errorMessage" class="error-message">{{ errorMessage }}</div>
          <div v-if="successMessage" class="success-message">{{ successMessage }}</div>
        </div>

        <div class="dialog-footer">
          <button @click="closeAddDialog" class="btn btn-secondary">取消</button>
          <button @click="addTasks" class="btn btn-primary" :disabled="urls.length === 0">
            添加 {{ urls.length > 0 ? `(${urls.length}个)` : '' }}
          </button>
        </div>
      </div>
    </div>

    <!-- 视频播放器 -->
    <VideoPlayer
      v-show="playerVisible"
      :visible="playerVisible"
      :src="playerSrc"
      :title="playerTitle"
      @close="handlePlayerClose"
    />
  </div>
</template>

<style scoped>
.download-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: #fff;
  border-radius: 12px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.06);
  overflow: hidden;
}

/* 任务区域 */
.task-section {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  background: #fafbfc;
  border-bottom: 1px solid #f0f0f0;
  flex-shrink: 0;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  color: #1a1a2e;
}

.tool-status {
  font-size: 12px;
  color: #94a3b8;
  padding: 4px 10px;
  background: #f3f4f6;
  border-radius: 12px;
  margin-left: 12px;
}

.tool-status.available {
  background: #dcfce7;
  color: #16a34a;
}

.filter-input {
  padding: 6px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 6px;
  font-size: 13px;
  width: 180px;
  transition: all 0.2s;
}

.filter-input:focus {
  outline: none;
  border-color: #667eea;
}

.filter-select {
  padding: 6px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 6px;
  font-size: 13px;
  background: white;
  cursor: pointer;
  transition: all 0.2s;
}

.filter-select:focus {
  outline: none;
  border-color: #667eea;
}

.cleanup-btn {
  padding: 6px 12px;
  background: transparent;
  color: #667eea;
  border: 1px solid #667eea;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.cleanup-btn:hover {
  background: #667eea;
  color: white;
}

.add-btn-small {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%);
  color: white;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.add-btn-small:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.4);
}

.add-btn-small:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 空状态 */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 60px 20px;
  text-align: center;
  flex: 1;
}

.empty-icon {
  color: #334155;
  margin-bottom: 16px;
}

.empty-text {
  font-size: 16px;
  color: #64748b;
  margin-bottom: 20px;
}

.go-add-btn {
  padding: 10px 24px;
  background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  cursor: pointer;
}

.go-add-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 表格 */
.task-table {
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

.empty-tip {
  padding: 40px 20px;
  text-align: center;
  color: #94a3b8;
  font-size: 13px;
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

.table-row.running {
  background: #f0f9ff;
}

/* 封面列 */
.col-cover {
  width: 60px;
  height: 34px;
  flex-shrink: 0;
  margin-right: 12px;
  position: relative;
  overflow: hidden;
  border-radius: 4px;
  background: #f5f5f5;
}

.cover-thumbnail {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.cover-placeholder-small {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f8f9fa;
  color: #cbd5e1;
}

/* 名称列 */
.col-name {
  flex: 1;
  min-width: 0;
  padding-right: 16px;
}

.task-title {
  display: block;
  font-size: 14px;
  font-weight: 500;
  color: #1a1a2e;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-url {
  display: block;
  font-size: 11px;
  color: #94a3b8;
  font-family: monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.task-progress-info {
  margin-top: 4px;
  display: flex;
  gap: 12px;
  font-size: 11px;
}

.progress-percent {
  color: #16a34a;
  font-weight: 600;
}

.progress-speed {
  color: #64748b;
}

.task-file-info {
  margin-top: 4px;
}

.file-path {
  font-size: 11px;
  color: #64748b;
  font-family: monospace;
  background: #f8f9fa;
  padding: 2px 6px;
  border-radius: 4px;
}

.task-error-info {
  margin-top: 4px;
}

.error-message {
  font-size: 11px;
  color: #dc2626;
}

/* 状态列 */
.col-status {
  width: 120px;
  padding-right: 16px;
}

.task-progress {
  width: 100%;
}

.progress-bar {
  height: 6px;
  background: #e5e7eb;
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, #22c55e, #16a34a);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.status-tag {
  display: inline-block;
  padding: 4px 10px;
  border-radius: 12px;
  font-size: 11px;
  font-weight: 500;
}

.status-pending { background: #fef3c7; color: #d97706; }
.status-queued { background: #dbeafe; color: #2563eb; }
.status-downloading { background: #dcfce7; color: #16a34a; }
.status-paused { background: #fef3c7; color: #d97706; }
.status-completed { background: #dcfce7; color: #16a34a; }
.status-failed { background: #fee2e2; color: #dc2626; }
.status-cancelled { background: #f3f4f6; color: #6b7280; }

/* 失败状态和错误提示 */
.status-error {
  position: relative;
  display: inline-block;
}

.error-tooltip {
  visibility: hidden;
  position: absolute;
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%);
  background: #1e293b;
  color: #fee2e2;
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 11px;
  white-space: nowrap;
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  z-index: 100;
  margin-bottom: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.status-error:hover .error-tooltip {
  visibility: visible;
}

/* 操作列 */
.col-action {
  width: 120px;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.action-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.action-btn.stop {
  background: #fee2e2;
  color: #dc2626;
}

.action-btn.stop:hover {
  background: #fecaca;
}

.action-btn.start {
  background: #dcfce7;
  color: #16a34a;
}

.action-btn.start:hover {
  background: #bbf7d0;
}

.action-btn.play {
  background: #e0e7ff;
  color: #4f46e5;
}

.action-btn.play:hover {
  background: #c7d2fe;
}

.action-btn.folder {
  background: #f3f4f6;
  color: #6b7280;
}

.action-btn.folder:hover {
  background: #e5e7eb;
  color: #374151;
}

.action-btn.delete {
  background: #f3f4f6;
  color: #6b7280;
}

.action-btn.delete:hover {
  background: #fee2e2;
  color: #dc2626;
}

/* 弹窗样式 */
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
}

.dialog {
  width: 90%;
  max-width: 500px;
  background: #1e293b;
  border-radius: 16px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 20px 24px;
  border-bottom: 1px solid #334155;
}

.dialog-header h3 {
  margin: 0;
  font-size: 18px;
  color: #f1f5f9;
}

.close-btn {
  background: transparent;
  border: none;
  color: #64748b;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
}

.close-btn:hover {
  color: #f1f5f9;
  background: #334155;
}

.dialog-body {
  padding: 24px;
}

.form-group {
  margin-bottom: 20px;
}

.form-group label {
  display: block;
  font-size: 14px;
  color: #94a3b8;
  margin-bottom: 8px;
}

.form-input {
  width: 100%;
  padding: 12px 16px;
  background: #0f172a;
  border: 1px solid #334155;
  border-radius: 8px;
  color: #f1f5f9;
  font-size: 14px;
}

.form-input:focus {
  outline: none;
  border-color: #6366f1;
}

.form-textarea {
  width: 100%;
  height: 100px;
  padding: 12px 16px;
  background: #0f172a;
  border: 1px solid #334155;
  border-radius: 8px;
  color: #f1f5f9;
  font-size: 14px;
  resize: vertical;
}

.form-textarea:focus {
  outline: none;
  border-color: #6366f1;
}

.url-list {
  max-height: 150px;
  overflow-y: auto;
  margin-bottom: 16px;
}

.url-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background: #0f172a;
  border-radius: 6px;
  margin-bottom: 8px;
}

.url-text {
  font-size: 12px;
  color: #94a3b8;
  word-break: break-all;
  flex: 1;
}

.remove-url {
  background: transparent;
  border: none;
  color: #64748b;
  cursor: pointer;
  padding: 4px;
  margin-left: 8px;
}

.remove-url:hover {
  color: #ef4444;
}

.error-message {
  padding: 12px;
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 8px;
  color: #ef4444;
  font-size: 14px;
  margin-bottom: 16px;
}

.success-message {
  padding: 12px;
  background: rgba(34, 197, 94, 0.1);
  border: 1px solid rgba(34, 197, 94, 0.3);
  border-radius: 8px;
  color: #22c55e;
  font-size: 14px;
  margin-bottom: 16px;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 16px 24px;
  border-top: 1px solid #334155;
}

.btn {
  padding: 10px 20px;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-secondary {
  background: transparent;
  color: #94a3b8;
  border: 1px solid #334155;
}

.btn-secondary:hover {
  background: #334155;
  color: #f1f5f9;
}

.btn-primary {
  background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%);
  color: white;
}

.btn-primary:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.4);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
