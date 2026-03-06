<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import type { ConvertTask, ConvertOptions } from '../types'
import { ConvertStatus } from '../types'
import { startConvert, selectConvertInput } from '../services/api'

// 输入文件
const inputPath = ref('')
const outputFormat = ref('mp4')
const isConverting = ref(false)

// 转换选项
const options = ref<ConvertOptions>({
  format: 'mp4',
  audio_only: false,
})

// 高级选项折叠
const showAdvanced = ref(false)

// 转换任务列表
const tasks = ref<ConvertTask[]>([])

// 预设模板
const presets = [
  { label: 'MP4 视频', format: 'mp4', icon: '🎬', audio_only: false },
  { label: 'MKV 视频', format: 'mkv', icon: '🎞️', audio_only: false },
  { label: 'WebM 视频', format: 'webm', icon: '🌐', audio_only: false },
  { label: 'MOV 视频', format: 'mov', icon: '🍎', audio_only: false },
  { label: 'GIF 动图', format: 'gif', icon: '🖼️', audio_only: false },
  { label: 'MP3 音频', format: 'mp3', icon: '🎵', audio_only: true },
  { label: 'M4A 音频', format: 'm4a', icon: '🎧', audio_only: true },
  { label: 'WAV 音频', format: 'wav', icon: '🔊', audio_only: true },
  { label: 'FLAC 无损', format: 'flac', icon: '💿', audio_only: true },
]

// 分辨率选项
const resolutions = [
  { label: '保持原始', value: '' },
  { label: '4K (3840×2160)', value: '3840x2160' },
  { label: '1080p (1920×1080)', value: '1920x1080' },
  { label: '720p (1280×720)', value: '1280x720' },
  { label: '480p (854×480)', value: '854x480' },
  { label: '360p (640×360)', value: '640x360' },
]

let unlistenProgress: (() => void) | null = null

onMounted(async () => {
  unlistenProgress = await listen<ConvertTask>('convert-progress', (event) => {
    const update = event.payload
    const idx = tasks.value.findIndex(t => t.id === update.id)
    if (idx !== -1) {
      tasks.value[idx] = update
    } else {
      tasks.value.unshift(update)
    }
  })
})

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress()
})

// 选择输入文件
async function selectInput() {
  try {
    const path = await selectConvertInput()
    if (path) {
      inputPath.value = path
    }
  } catch (e) {
    console.error('选择文件失败:', e)
  }
}

// 选择预设
function selectPreset(preset: typeof presets[0]) {
  outputFormat.value = preset.format
  options.value.format = preset.format
  options.value.audio_only = preset.audio_only
  if (preset.audio_only) {
    options.value.video_codec = undefined
    options.value.resolution = undefined
    options.value.video_bitrate = undefined
    options.value.fps = undefined
  }
}

// 开始转换
async function startConversion() {
  if (!inputPath.value) {
    return
  }

  isConverting.value = true
  options.value.format = outputFormat.value

  try {
    const result = await startConvert(inputPath.value, null, options.value)
    // 任务通过 event 监听更新
    const idx = tasks.value.findIndex(t => t.id === result.id)
    if (idx !== -1) {
      tasks.value[idx] = result
    } else {
      tasks.value.unshift(result)
    }
  } catch (e) {
    alert('转换失败: ' + e)
  } finally {
    isConverting.value = false
  }
}

// 清除已完成任务
function clearCompleted() {
  tasks.value = tasks.value.filter(t => t.status !== ConvertStatus.Completed && t.status !== ConvertStatus.Failed)
}

// 获取状态文本
function getStatusText(status: ConvertStatus): string {
  const map: Record<string, string> = {
    Pending: '等待中',
    Converting: '转换中',
    Completed: '已完成',
    Failed: '失败',
  }
  return map[status] || status
}

// 获取状态样式
function getStatusClass(status: ConvertStatus): string {
  const map: Record<string, string> = {
    Pending: 'bg-amber-100 text-amber-700',
    Converting: 'bg-blue-100 text-blue-700',
    Completed: 'bg-green-100 text-green-700',
    Failed: 'bg-red-100 text-red-700',
  }
  return map[status] || ''
}

// 获取文件名
function getFileName(path: string): string {
  return path.split('/').pop()?.split('\\').pop() || path
}
</script>

<template>
  <div class="h-full flex flex-col bg-white dark:bg-gray-900 rounded-xl shadow-[0_2px_12px_rgba(0,0,0,0.06)] overflow-hidden">
    <!-- 转换配置区 -->
    <div class="px-5 py-4 border-b border-[#f0f0f0] dark:border-gray-700 shrink-0">
      <!-- 输入文件选择 -->
      <div class="flex gap-2.5 mb-4">
        <div class="flex-1 flex items-center gap-2 px-3.5 py-2.5 bg-[#f5f6f8] dark:bg-gray-800 rounded-lg">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-[#94a3b8] shrink-0">
            <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
            <polyline points="13 2 13 9 20 9" />
          </svg>
          <input
            type="text"
            v-model="inputPath"
            placeholder="选择要转换的文件..."
            readonly
            class="flex-1 border-none bg-transparent text-sm outline-none text-[#1a1a2e] dark:text-gray-200 placeholder:text-[#94a3b8] cursor-pointer"
            @click="selectInput"
          />
        </div>
        <button
          @click="selectInput"
          class="px-4 py-2.5 border border-[#e5e7eb] dark:border-gray-600 bg-white dark:bg-gray-800 text-[#64748b] dark:text-gray-300 rounded-lg text-sm cursor-pointer transition-all hover:bg-[#f5f6f8] dark:hover:bg-gray-700"
        >浏览</button>
      </div>

      <!-- 格式预设网格 -->
      <div class="grid grid-cols-9 gap-1.5 mb-4">
        <button
          v-for="preset in presets"
          :key="preset.format"
          :class="[
            'flex flex-col items-center gap-1 py-2 px-1 rounded-lg border text-center cursor-pointer transition-all',
            outputFormat === preset.format
              ? 'border-[#667eea] bg-[#eef2ff] dark:bg-indigo-900/20 dark:border-indigo-500'
              : 'border-[#e5e7eb] dark:border-gray-700 bg-white dark:bg-gray-800 hover:border-[#d1d5db] dark:hover:border-gray-600'
          ]"
          @click="selectPreset(preset)"
        >
          <span class="text-base">{{ preset.icon }}</span>
          <span :class="['text-[10px] font-medium', outputFormat === preset.format ? 'text-[#667eea] dark:text-indigo-400' : 'text-[#64748b] dark:text-gray-400']">{{ preset.label }}</span>
        </button>
      </div>

      <!-- 高级选项 -->
      <div>
        <button
          @click="showAdvanced = !showAdvanced"
          class="flex items-center gap-1 text-xs text-[#94a3b8] bg-transparent border-none cursor-pointer hover:text-[#667eea] mb-2"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" :class="['transition-transform', showAdvanced ? 'rotate-90' : '']">
            <polyline points="9 18 15 12 9 6" />
          </svg>
          高级选项
        </button>

        <div v-if="showAdvanced" class="grid grid-cols-4 gap-3">
          <div class="flex flex-col gap-1">
            <label class="text-[11px] font-medium text-[#64748b] dark:text-gray-400">分辨率</label>
            <select v-model="options.resolution" class="select-modern px-2 py-1.5 border border-[#e5e7eb] dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 rounded text-[12px] bg-white">
              <option v-for="r in resolutions" :key="r.value" :value="r.value || undefined">{{ r.label }}</option>
            </select>
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[11px] font-medium text-[#64748b] dark:text-gray-400">视频码率(kbps)</label>
            <input type="number" v-model="options.video_bitrate" placeholder="自动" class="px-2 py-1.5 border border-[#e5e7eb] dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 rounded text-[12px]" />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[11px] font-medium text-[#64748b] dark:text-gray-400">音频码率(kbps)</label>
            <input type="number" v-model="options.audio_bitrate" placeholder="自动" class="px-2 py-1.5 border border-[#e5e7eb] dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 rounded text-[12px]" />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[11px] font-medium text-[#64748b] dark:text-gray-400">帧率</label>
            <input type="number" v-model="options.fps" placeholder="自动" class="px-2 py-1.5 border border-[#e5e7eb] dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 rounded text-[12px]" />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[11px] font-medium text-[#64748b] dark:text-gray-400">开始时间(秒)</label>
            <input type="number" v-model="options.start_time" placeholder="从头" step="0.1" class="px-2 py-1.5 border border-[#e5e7eb] dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 rounded text-[12px]" />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[11px] font-medium text-[#64748b] dark:text-gray-400">结束时间(秒)</label>
            <input type="number" v-model="options.end_time" placeholder="到尾" step="0.1" class="px-2 py-1.5 border border-[#e5e7eb] dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 rounded text-[12px]" />
          </div>
        </div>
      </div>

      <!-- 开始转换按钮 -->
      <div class="flex justify-end mt-3">
        <button
          @click="startConversion"
          :disabled="isConverting || !inputPath"
          class="px-8 py-2.5 text-white border-none rounded-lg text-sm font-medium cursor-pointer whitespace-nowrap transition-all bg-[linear-gradient(135deg,#667eea_0%,#764ba2_100%)] hover:-translate-y-0.5 hover:shadow-[0_4px_12px_rgba(102,126,234,0.35)] disabled:opacity-60 disabled:cursor-not-allowed"
        >
          {{ isConverting ? '转换中...' : '开始转换' }}
        </button>
      </div>
    </div>

    <!-- 任务列表 -->
    <div class="flex-1 flex flex-col overflow-hidden">
      <div class="flex justify-between items-center px-5 py-2.5 bg-[#fafbfc] dark:bg-gray-800 border-b border-[#f0f0f0] dark:border-gray-700 shrink-0">
        <span class="text-xs font-semibold text-[#64748b]">转换记录 ({{ tasks.length }})</span>
        <button
          v-if="tasks.some(t => t.status === 'Completed' || t.status === 'Failed')"
          @click="clearCompleted"
          class="text-[10px] text-[#94a3b8] bg-transparent border-none cursor-pointer hover:text-[#667eea]"
        >清除已完成</button>
      </div>

      <div class="flex-1 overflow-y-auto">
        <div v-if="tasks.length === 0" class="flex flex-col items-center justify-center py-10 text-[#94a3b8]">
          <p class="text-sm">暂无转换记录</p>
        </div>

        <div v-for="task in tasks" :key="task.id" class="flex items-center gap-3 px-5 py-3 border-b border-[#f5f5f5] dark:border-gray-800">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium text-[#1a1a2e] dark:text-gray-200 whitespace-nowrap overflow-hidden text-ellipsis">
              {{ getFileName(task.input_path) }}
            </p>
            <p class="text-[11px] text-[#94a3b8] mt-0.5">
              → {{ task.output_format.toUpperCase() }}
              <span v-if="task.status === 'Completed'" class="text-[#22c55e]"> | {{ getFileName(task.output_path) }}</span>
            </p>
            <div v-if="task.status === 'Converting'" class="mt-1.5">
              <div class="h-1.5 bg-[#e5e7eb] dark:bg-gray-700 rounded-[3px] overflow-hidden">
                <div class="h-full rounded-[3px] transition-all duration-300 bg-[linear-gradient(90deg,#667eea,#764ba2)]" :style="{ width: task.progress + '%' }"></div>
              </div>
              <div class="flex justify-between mt-0.5">
                <span class="text-[10px] text-[#667eea] font-semibold">{{ task.progress }}%</span>
                <span class="text-[10px] text-[#94a3b8]">{{ task.message }}</span>
              </div>
            </div>
          </div>
          <span :class="['inline-block px-2.5 py-1 rounded-full text-[11px] font-medium shrink-0', getStatusClass(task.status)]">
            {{ getStatusText(task.status) }}
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
</style>
