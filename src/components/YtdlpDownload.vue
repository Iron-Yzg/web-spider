<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { YtdlpTask, YtdlpTaskStatus } from '../types'

// 任务列表
const tasks = ref<YtdlpTask[]>([])

// yt-dlp 状态
const ytdlpAvailable = ref(false)
const ytdlpVersion = ref('')

// 下载配置
const downloadPath = ref('./downloads')

// 弹窗状态
const showAddDialog = ref(false)
const urlInput = ref('')
const urls = ref<string[]>([])
const errorMessage = ref('')
const successMessage = ref('')

// 监听器
let unlistenProgress: (() => void) | null = null

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
  try {
    tasks.value = await invoke<YtdlpTask[]>('get_ytdlp_tasks')
  } catch (e) {
    console.error('加载任务失败:', e)
  }

  // 监听进度
  unlistenProgress = await listen<YtdlpTask>('ytdlp-progress', async (event: { payload: YtdlpTask }) => {
    const task = event.payload
    const index = tasks.value.findIndex(t => t.id === task.id)
    if (index !== -1) {
      // 更新现有任务
      tasks.value[index] = task
    } else {
      // 任务可能还没添加到列表，尝试添加到本地
      tasks.value.push(task)
      // 同时从后端刷新任务列表以确保数据一致
      try {
        const allTasks = await invoke<YtdlpTask[]>('get_ytdlp_tasks')
        // 合并任务，保持前端状态但更新数据
        for (const t of allTasks) {
          const existingIdx = tasks.value.findIndex(st => st.id === t.id)
          if (existingIdx !== -1) {
            // 如果任务在本地有更新（如下进度），保留本地版本
            if (tasks.value[existingIdx].progress >= t.progress) {
              tasks.value[existingIdx] = tasks.value[existingIdx]
            } else {
              tasks.value[existingIdx] = t
            }
          } else {
            tasks.value.push(t)
          }
        }
      } catch (e) {
        console.error('刷新任务列表失败:', e)
      }
    }
  })
})

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress()
})

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

// 添加 URL
function addUrl() {
  const url = urlInput.value.trim()
  if (url && isValidUrl(url) && !urls.value.includes(url)) {
    urls.value.push(url)
    urlInput.value = ''
  }
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

// 批量添加 URL（支持多行/逗号）
function parseMultipleUrls(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text') || ''
  const lines = text.split(/[\n,]/).map(u => u.trim()).filter(u => u)
  for (const url of lines) {
    if (isValidUrl(url) && !urls.value.includes(url)) {
      urls.value.push(url)
    }
  }
}

// 开始下载（只添加任务，不启动）
async function startDownload() {
  if (urls.value.length === 0) {
    errorMessage.value = '请输入视频链接'
    return
  }

  errorMessage.value = ''
  successMessage.value = ''

  try {
    // 只添加任务到数据库，不启动下载
    await invoke('add_ytdlp_tasks', {
      urls: urls.value,
      outputPath: downloadPath.value,
    })

    successMessage.value = `已添加 ${urls.value.length} 个任务`
  } catch (e) {
    errorMessage.value = '添加任务失败: ' + e
  } finally {
    // 先关闭弹窗
    closeAddDialog()
    // 刷新任务列表
    try {
      tasks.value = await invoke<YtdlpTask[]>('get_ytdlp_tasks')
    } catch (e) {
      console.error('刷新任务列表失败:', e)
    }
  }
}

// 停止任务
async function stopTask(taskId: string) {
  try {
    await invoke('stop_ytdlp_task', { taskId: taskId })
    const index = tasks.value.findIndex(t => t.id === taskId)
    if (index !== -1) {
      tasks.value[index].status = 'Paused' as YtdlpTaskStatus
      tasks.value[index].message = '已暂停'
    }
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
  } catch (e) {
    console.error('开始任务失败:', e)
  }
}

// 删除任务
async function deleteTask(taskId: string) {
  try {
    await invoke('delete_ytdlp_task', { taskId: taskId })
    tasks.value = tasks.value.filter(t => t.id !== taskId)
  } catch (e) {
    console.error('删除任务失败:', e)
  }
}

// 清理完成的任务
async function cleanupTasks() {
  try {
    await invoke('cleanup_ytdlp_tasks')
    tasks.value = tasks.value.filter(t =>
      t.status === YtdlpTaskStatus.Downloading ||
      t.status === YtdlpTaskStatus.Pending ||
      t.status === YtdlpTaskStatus.Queued ||
      t.status === YtdlpTaskStatus.Paused
    )
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

// 获取状态样式
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
</script>

<template>
  <div class="download-page">
    <!-- 工具状态 -->
    <div class="tool-status" :class="{ available: ytdlpAvailable }">
      <div class="status-icon">
        <svg v-if="ytdlpAvailable" xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="20 6 9 17 4 12"></polyline>
        </svg>
        <svg v-else xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="10"></circle>
          <line x1="15" y1="9" x2="9" y2="15"></line>
          <line x1="9" y1="9" x2="15" y2="15"></line>
        </svg>
      </div>
      <div class="status-text">
        <span v-if="ytdlpAvailable">yt-dlp 已就绪 ({{ ytdlpVersion }})</span>
        <span v-else>yt-dlp 未安装或配置错误</span>
      </div>
      <button class="add-btn" @click="openAddDialog" :disabled="!ytdlpAvailable">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="12" y1="5" x2="12" y2="19"></line>
          <line x1="5" y1="12" x2="19" y2="12"></line>
        </svg>
        添加下载
      </button>
    </div>

    <!-- 任务列表 -->
    <div class="tasks-section">
      <div class="tasks-header">
        <span class="tasks-count">共 {{ tasks.length }} 个任务</span>
        <button @click="cleanupTasks" :disabled="tasks.filter(t => t.status === 'Completed' || t.status === 'Failed' || t.status === 'Cancelled').length === 0" class="cleanup-btn">
          清理已完成
        </button>
      </div>

      <div v-if="tasks.length === 0" class="empty-tasks">
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
          <polyline points="7 10 12 15 17 10"></polyline>
          <line x1="12" y1="15" x2="12" y2="3"></line>
        </svg>
        <p>暂无下载任务</p>
        <button @click="openAddDialog" class="go-add-btn" :disabled="!ytdlpAvailable">添加下载</button>
      </div>

      <div v-else class="tasks-list">
        <div v-for="task in tasks" :key="task.id" class="task-item" :class="getStatusClass(task.status)">
          <div class="task-info">
            <div class="task-title">{{ task.title || task.url }}</div>
            <div class="task-meta">
              <span class="task-status" :class="getStatusClass(task.status)">{{ getStatusText(task.status) }}</span>
            </div>
          </div>

          <!-- 下载中显示进度条 -->
          <div v-if="task.status === 'Downloading'" class="task-progress">
            <div class="progress-bar">
              <div class="progress-fill" :style="{ width: task.progress + '%' }"></div>
            </div>
            <div class="progress-info">
              <span class="progress-percent">{{ Math.round(task.progress) }}%</span>
              <span v-if="task.speed" class="progress-speed">{{ task.speed }}</span>
            </div>
          </div>

          <!-- 显示文件路径（如果有） -->
          <div v-if="task.file_path" class="task-file-path">
            {{ task.file_path }}
          </div>

          <!-- 任务操作 -->
          <div class="task-actions">
            <button
              v-if="task.status === 'Downloading' || task.status === 'Pending' || task.status === 'Queued'"
              @click="stopTask(task.id)"
              class="action-btn stop-btn"
            >
              停止
            </button>
            <button
              v-if="task.status !== 'Completed'"
              @click="startTask(task.id)"
              class="action-btn start-btn"
            >
              开始
            </button>
            <button
              v-if="task.status === 'Completed' || task.status === 'Failed' || task.status === 'Cancelled' || task.status === 'Paused'"
              @click="deleteTask(task.id)"
              class="action-btn delete-btn"
            >
              删除
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 添加下载弹窗 -->
    <div v-if="showAddDialog" class="dialog-overlay" @click.self="closeAddDialog">
      <div class="dialog">
        <div class="dialog-header">
          <h3>添加下载任务</h3>
          <button class="close-btn" @click="closeAddDialog">&times;</button>
        </div>
        <div class="dialog-content">
          <!-- 错误/成功提示 -->
          <div v-if="errorMessage" class="alert error">{{ errorMessage }}</div>
          <div v-if="successMessage" class="alert success">{{ successMessage }}</div>

          <!-- URL 输入区 -->
          <div class="url-section">
            <label>视频链接</label>
            <textarea
              v-model="urlInput"
              @paste="parseMultipleUrls($event)"
              placeholder="粘贴视频链接（支持 YouTube、B站等）&#10;多个链接可以用换行或逗号分隔"
              rows="4"
            ></textarea>
            <div class="url-actions">
              <button @click="addUrl" :disabled="!urlInput.trim()" class="add-url-btn">
                添加链接
              </button>
              <button @click="urls = []" :disabled="urls.length === 0" class="clear-btn">
                清空
              </button>
            </div>

            <!-- 已添加的链接列表 -->
            <div v-if="urls.length > 0" class="url-list">
              <div v-for="(url, index) in urls" :key="index" class="url-item">
                <span class="url-text">{{ url }}</span>
                <button @click="removeUrl(index)" class="remove-url">&times;</button>
              </div>
            </div>
          </div>
        </div>
        <div class="dialog-footer">
          <button @click="closeAddDialog" class="cancel-btn">取消</button>
          <button @click="startDownload" :disabled="urls.length === 0" class="start-btn">
            开始下载 ({{ urls.length }} 个)
          </button>
        </div>
      </div>
    </div>
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

/* 工具状态 */
.tool-status {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 20px;
  background: #fef2f2;
  border-bottom: 1px solid #fecaca;
}

.tool-status.available {
  background: #f0fdf4;
  border-color: #bbf7d0;
}

.status-icon {
  display: flex;
  align-items: center;
}

.tool-status.available .status-icon {
  color: #22c55e;
}

.tool-status:not(.available) .status-icon {
  color: #ef4444;
}

.status-text {
  flex: 1;
  font-size: 13px;
  color: #64748b;
}

.add-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.add-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.add-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 任务列表 */
.tasks-section {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
}

.tasks-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.tasks-count {
  font-size: 13px;
  color: #64748b;
}

.cleanup-btn {
  padding: 6px 12px;
  background: #f1f5f9;
  color: #64748b;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
}

.cleanup-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.empty-tasks {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 60px 20px;
  color: #94a3b8;
  text-align: center;
}

.empty-tasks svg {
  margin-bottom: 12px;
  opacity: 0.5;
}

.empty-tasks p {
  font-size: 14px;
  margin-bottom: 16px;
}

.go-add-btn {
  padding: 8px 20px;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
}

.go-add-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.tasks-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.task-item {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 16px;
  background: #fafbfc;
  border-radius: 8px;
  border-left: 3px solid #e5e7eb;
}

.task-item.status-downloading {
  border-left-color: #667eea;
  background: #f0f4ff;
}

.task-item.status-completed {
  border-left-color: #22c55e;
}

.task-item.status-failed {
  border-left-color: #ef4444;
}

.task-item.status-cancelled {
  border-left-color: #94a3b8;
}

.task-item.status-paused {
  border-left-color: #f59e0b;
  background: #fffbeb;
}

.task-info {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}

.task-title {
  font-size: 13px;
  font-weight: 500;
  color: #1a1a2e;
  word-break: break-all;
  flex: 1;
}

.task-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: 12px;
}

.task-status {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
}

.task-status.status-pending,
.task-status.status-queued {
  background: #fef3c7;
  color: #d97706;
}

.task-status.status-downloading {
  background: #dbeafe;
  color: #2563eb;
}

.task-status.status-completed {
  background: #dcfce7;
  color: #16a34a;
}

.task-status.status-failed {
  background: #fee2e2;
  color: #dc2626;
}

.task-status.status-cancelled {
  background: #f1f5f9;
  color: #64748b;
}

.task-status.status-paused {
  background: #fef3c7;
  color: #d97706;
}

.task-size {
  font-size: 11px;
  color: #94a3b8;
}

.task-file-path {
  font-size: 11px;
  color: #94a3b8;
  word-break: break-all;
  margin-top: 4px;
}

/* 进度条 */
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
  background: linear-gradient(90deg, #667eea, #764ba2);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.progress-info {
  display: flex;
  gap: 12px;
  margin-top: 4px;
  font-size: 11px;
  color: #64748b;
}

.progress-speed {
  color: #667eea;
  font-weight: 500;
}

/* 任务操作 */
.task-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.action-btn {
  padding: 4px 12px;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.stop-btn {
  background: #fee2e2;
  color: #dc2626;
}

.stop-btn:hover {
  background: #fecaca;
}

.start-btn {
  background: #dcfce7;
  color: #16a34a;
}

.start-btn:hover {
  background: #bbf7d0;
}

.cancel-btn {
  background: #fee2e2;
  color: #dc2626;
}

.cancel-btn:hover {
  background: #fecaca;
}

.redownload-btn {
  background: #dbeafe;
  color: #2563eb;
}

.redownload-btn:hover {
  background: #bfdbfe;
}

.delete-btn {
  background: #fee2e2;
  color: #dc2626;
}

.delete-btn:hover {
  background: #fecaca;
}

/* 弹窗 */
.dialog-overlay {
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

.dialog {
  background: white;
  border-radius: 12px;
  width: 500px;
  max-width: 90vw;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 40px rgba(0, 0, 0, 0.2);
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid #f0f0f0;
}

.dialog-header h3 {
  font-size: 16px;
  font-weight: 600;
  color: #1a1a2e;
}

.close-btn {
  background: none;
  border: none;
  font-size: 24px;
  color: #94a3b8;
  cursor: pointer;
  line-height: 1;
}

.close-btn:hover {
  color: #1a1a2e;
}

.dialog-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 16px 20px;
  border-top: 1px solid #f0f0f0;
}

.dialog-footer .cancel-btn {
  background: #f1f5f9;
  color: #64748b;
  padding: 8px 16px;
  border-radius: 6px;
}

.dialog-footer .start-btn {
  padding: 8px 20px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
}

.dialog-footer .start-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 弹窗内的表单 */
.alert {
  padding: 12px 16px;
  border-radius: 8px;
  margin-bottom: 16px;
  font-size: 13px;
}

.alert.error {
  background: #fef2f2;
  color: #dc2626;
  border: 1px solid #fecaca;
}

.alert.success {
  background: #f0fdf4;
  color: #16a34a;
  border: 1px solid #bbf7d0;
}

.url-section {
  margin-bottom: 0;
}

.url-section label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  color: #374151;
  margin-bottom: 8px;
}

.url-section textarea {
  width: 100%;
  padding: 12px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  font-size: 13px;
  resize: vertical;
  font-family: inherit;
}

.url-section textarea:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.url-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.add-url-btn {
  padding: 8px 16px;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
}

.add-url-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.url-list {
  margin-top: 12px;
}

.url-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: #f8fafc;
  border-radius: 6px;
  margin-bottom: 4px;
}

.url-text {
  font-size: 12px;
  color: #64748b;
  word-break: break-all;
}

.remove-url {
  padding: 4px 8px;
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  font-size: 16px;
}

.remove-url:hover {
  color: #ef4444;
}
</style>
