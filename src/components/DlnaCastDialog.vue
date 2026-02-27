<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import QRCode from 'qrcode'
import type { LocalVideo } from '../types'
import type { CastDevice, CastProtocol, CastPlaylistItem } from '../services/api'
import { 
  discoverCastDevices,
  castMedia,
  stopCastPlayback,
  createCastRemoteSession,
  getLocalIpAddress,
  startDlnaMediaServer,
  stopDlnaMediaServer,
} from '../services/api'

const props = defineProps<{
  video: LocalVideo
  playlist?: LocalVideo[]
  currentIndex?: number
}>()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const isDiscovering = ref(false)
const devices = ref<CastDevice[]>([])
const localIp = ref('')
const serverUrl = ref('')
const isStartingServer = ref(false)
const isCasting = ref(false)
const statusMessage = ref('')
const selectedDevice = ref<string | null>(null)
const castDeviceName = ref<string | null>(null)
const selectedProtocol = ref<CastProtocol>('sony')
const managedServer = ref(false)
const remoteUrl = ref('')
const remoteQrDataUrl = ref('')
const isCreatingRemote = ref(false)

const DEVICE_CACHE_KEY = 'cast.devices.cache.v1'
const SELECTED_DEVICE_KEY = 'cast.selected.device.v1'
const SELECTED_PROTOCOL_KEY = 'cast.selected.protocol.v1'

onMounted(async () => {
  const cachedProtocol = localStorage.getItem(SELECTED_PROTOCOL_KEY) as CastProtocol | null
  if (cachedProtocol) {
    selectedProtocol.value = cachedProtocol
  }

  const cachedSelectedDevice = localStorage.getItem(SELECTED_DEVICE_KEY)
  if (cachedSelectedDevice) {
    selectedDevice.value = cachedSelectedDevice
  }

  const cacheRaw = localStorage.getItem(DEVICE_CACHE_KEY)
  if (cacheRaw) {
    try {
      const cachedDevices = JSON.parse(cacheRaw) as CastDevice[]
      if (Array.isArray(cachedDevices) && cachedDevices.length > 0) {
        devices.value = cachedDevices
        if (selectedDevice.value && !cachedDevices.some(d => d.id === selectedDevice.value && d.available)) {
          selectedDevice.value = null
        }
        statusMessage.value = `使用缓存设备列表（${cachedDevices.length}）`
      }
    } catch {
      localStorage.removeItem(DEVICE_CACHE_KEY)
    }
  }

  await handleStartServer()
  if (devices.value.length === 0) {
    await handleDiscover()
  }
})

async function handleDiscover() {
  isDiscovering.value = true
  statusMessage.value = '正在搜索设备...'
  devices.value = []
  
  try {
    const [ip, foundDevices] = await Promise.all([
      getLocalIpAddress(),
      discoverCastDevices(selectedProtocol.value, 5)
    ])
    localIp.value = ip
    devices.value = foundDevices

    localStorage.setItem(SELECTED_PROTOCOL_KEY, selectedProtocol.value)
    localStorage.setItem(DEVICE_CACHE_KEY, JSON.stringify(foundDevices))

    if (selectedDevice.value && !foundDevices.some(d => d.id === selectedDevice.value && d.available)) {
      selectedDevice.value = null
    }
    
    if (foundDevices.length === 0) {
      statusMessage.value = '未发现可用设备'
    } else {
      if (!selectedDevice.value) {
        const firstAvailable = foundDevices.find(d => d.available)
        if (firstAvailable) {
          selectedDevice.value = firstAvailable.id
          localStorage.setItem(SELECTED_DEVICE_KEY, firstAvailable.id)
        }
      }
      statusMessage.value = `发现 ${foundDevices.length} 个设备`
    }
  } catch (e) {
    statusMessage.value = `搜索失败: ${e}`
  } finally {
    isDiscovering.value = false
  }
}

const isLocalVideo = computed(() => {
  return props.video.file_path && props.video.file_path.length > 0
})

const statusIsError = computed(() => {
  return statusMessage.value.includes('失败') || statusMessage.value.includes('错误')
})

async function handleStartServer() {
  if (!props.video) return
  
  isStartingServer.value = true

  statusMessage.value = '正在启动媒体服务器...'
  
  try {
    const source = isLocalVideo.value ? props.video.file_path : (props.video.m3u8_url || '')
    if (!source) {
      throw new Error('未找到可投屏的视频地址')
    }
    const url = await startDlnaMediaServer(source, 0)
    managedServer.value = true
    serverUrl.value = url
    statusMessage.value = isLocalVideo.value
      ? `本地媒体服务已启动: ${url}`
      : `HLS 代理已启动: ${url}`
  } catch (e) {
    statusMessage.value = `启动服务器失败: ${e}`
  } finally {
    isStartingServer.value = false
  }
}

async function handleCast() {
  if (!selectedDevice.value) {
    statusMessage.value = '请选择投屏设备'
    return
  }
  
  isCasting.value = true
  statusMessage.value = '正在投屏...'
  
  try {
    castDeviceName.value = selectedDevice.value
    localStorage.setItem(SELECTED_DEVICE_KEY, selectedDevice.value)
    localStorage.setItem(SELECTED_PROTOCOL_KEY, selectedProtocol.value)
    await castMedia(selectedProtocol.value, selectedDevice.value, serverUrl.value, props.video.name)
    statusMessage.value = '已发送播放命令，请查看电视'
  } catch (e) {
    statusMessage.value = `投屏失败: ${e}`
  } finally {
    isCasting.value = false
  }
}

const remotePlaylistItems = computed<CastPlaylistItem[]>(() => {
  const list = props.playlist && props.playlist.length > 0 ? props.playlist : [props.video]
  return list
    .map(item => {
      const source = (item.file_path && item.file_path.trim().length > 0)
        ? item.file_path
        : (item.m3u8_url || '')
      if (!source) return null
      return {
        id: item.id,
        title: item.name,
        source,
      }
    })
    .filter((item): item is CastPlaylistItem => item !== null)
})

const remoteCurrentIndex = computed(() => {
  if (remotePlaylistItems.value.length === 0) return 0
  const idx = remotePlaylistItems.value.findIndex(item => item.id === props.video.id)
  if (idx >= 0) return idx
  if (typeof props.currentIndex === 'number' && props.currentIndex >= 0 && props.currentIndex < remotePlaylistItems.value.length) {
    return props.currentIndex
  }
  return 0
})

async function handleCreateRemoteQr() {
  if (!selectedDevice.value) {
    statusMessage.value = '请先选择投屏设备'
    return
  }
  if (remotePlaylistItems.value.length === 0) {
    statusMessage.value = '当前列表没有可投屏的视频'
    return
  }

  isCreatingRemote.value = true
  try {
    const url = await createCastRemoteSession(
      selectedDevice.value,
      remotePlaylistItems.value,
      remoteCurrentIndex.value,
    )
    remoteUrl.value = url
    remoteQrDataUrl.value = await QRCode.toDataURL(url, {
      width: 180,
      margin: 1,
      color: { dark: '#1a1a2e', light: '#ffffff' },
    })
  } catch (e) {
    statusMessage.value = `生成遥控二维码失败: ${e}`
  } finally {
    isCreatingRemote.value = false
  }
}

async function handleClose() {
  // 异步停止电视播放
  if (castDeviceName.value) {
    stopCastPlayback(selectedProtocol.value, castDeviceName.value).catch(e => console.error('停止播放失败:', e))
  }
  
  // 本地文件与网络 HLS 代理都需要停止服务器
  if (managedServer.value && serverUrl.value) {
    stopDlnaMediaServer().catch(e => console.error('停止服务器失败:', e))
  }
  
  emit('close')
}

function protocolLabel(protocol: string): string {
  if (protocol === 'dlna') return 'DLNA'
  if (protocol === 'chromecast') return 'Chromecast'
  if (protocol === 'airplay') return 'AirPlay'
  return protocol
}
</script>

<template>
  <div class="fixed inset-0 z-[1000] flex items-center justify-center bg-black/55 p-4">
    <div class="flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl">
      <div class="flex items-center justify-between border-b border-slate-200 px-4 py-3">
        <div>
          <h3 class="text-base font-semibold text-slate-900">一键投屏</h3>
          <p class="mt-0.5 line-clamp-1 text-xs text-slate-500">{{ video.name }}</p>
        </div>
        <button class="rounded-lg p-2 text-slate-500 transition hover:bg-slate-100 hover:text-slate-800" @click="handleClose">
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="flex-1 space-y-3 overflow-y-auto px-4 py-3">
        <div class="grid grid-cols-1 gap-2 sm:grid-cols-[1fr_auto]">
          <select
            v-model="selectedProtocol"
            class="h-10 rounded-xl border border-slate-200 bg-white px-3 text-sm text-slate-700 outline-none transition focus:border-indigo-400 focus:ring-2 focus:ring-indigo-100"
          >
            <option value="sony">Sony 优先（推荐）</option>
            <option value="auto">自动</option>
            <option value="dlna">DLNA</option>
            <option value="chromecast">Chromecast（实验）</option>
            <option value="airplay">AirPlay（实验）</option>
          </select>
          <button
            class="inline-flex h-10 items-center justify-center rounded-xl border border-slate-200 bg-slate-50 px-3 text-sm font-medium text-slate-700 transition hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60"
            @click="handleDiscover"
            :disabled="isDiscovering"
          >
            {{ isDiscovering ? '搜索中...' : '刷新设备' }}
          </button>
        </div>

        <div v-if="devices.length > 0" class="rounded-xl border border-slate-200 bg-slate-50/60 p-2">
          <div class="mb-2 px-1 text-xs font-medium text-slate-500">选择投屏设备</div>
          <div class="grid max-h-48 grid-cols-1 gap-2 overflow-y-auto pr-1 sm:grid-cols-2">
            <button
              v-for="device in devices"
              :key="device.id"
              class="rounded-xl border px-3 py-2 text-left transition"
              :class="[
                selectedDevice === device.id
                  ? 'border-indigo-500 bg-indigo-50 text-indigo-700'
                  : 'border-slate-200 bg-white text-slate-700 hover:border-slate-300',
                !device.available ? 'cursor-not-allowed opacity-60' : ''
              ]"
              @click="device.available ? selectedDevice = device.id : null"
            >
              <div class="line-clamp-1 text-sm font-medium">{{ device.name }}</div>
              <div class="mt-1 text-xs text-slate-500">
                {{ protocolLabel(device.protocol) }}
                <span v-if="device.note"> · {{ device.note }}</span>
              </div>
            </button>
          </div>
        </div>

        <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
          <button
            class="inline-flex h-10 items-center justify-center rounded-xl border border-indigo-200 bg-indigo-50 px-3 text-sm font-medium text-indigo-700 transition hover:bg-indigo-100 disabled:cursor-not-allowed disabled:opacity-60"
            @click="handleCreateRemoteQr"
            :disabled="isCreatingRemote || !selectedDevice"
          >
            {{ isCreatingRemote ? '生成中...' : '手机遥控' }}
          </button>
          <div class="flex items-center rounded-xl border border-slate-200 px-3 text-xs text-slate-500">
            {{ isLocalVideo ? '本地文件投屏' : '网络视频投屏' }}
          </div>
        </div>

        <div v-if="remoteQrDataUrl" class="rounded-xl border border-slate-200 bg-slate-50 p-3">
          <div class="flex flex-col items-center gap-2 sm:flex-row sm:items-start">
            <img :src="remoteQrDataUrl" alt="手机遥控二维码" class="h-36 w-36 rounded-lg border border-slate-200 bg-white" />
            <div class="min-w-0 flex-1">
              <p class="text-xs leading-5 text-slate-600">手机和电脑在同一局域网，扫码后可在手机上切换视频。</p>
              <code class="mt-2 block break-all rounded-md border border-slate-200 bg-white p-2 text-[11px] text-indigo-700">{{ remoteUrl }}</code>
            </div>
          </div>
        </div>

        <details v-if="serverUrl" class="rounded-xl border border-slate-200 bg-white px-3 py-2 text-xs text-slate-600">
          <summary class="cursor-pointer select-none">查看 {{ isLocalVideo ? '媒体服务地址' : '视频地址' }}</summary>
          <code class="mt-2 block break-all text-[11px] text-emerald-700">{{ serverUrl }}</code>
        </details>

        <div
          v-if="statusMessage"
          class="rounded-xl px-3 py-2 text-xs"
          :class="statusIsError ? 'bg-red-50 text-red-600' : 'bg-emerald-50 text-emerald-600'"
        >
          {{ statusMessage }}
        </div>
      </div>

      <div class="grid grid-cols-2 gap-2 border-t border-slate-200 px-4 py-3">
        <button class="h-10 rounded-xl border border-slate-200 bg-white text-sm font-medium text-slate-600 transition hover:bg-slate-50" @click="handleClose">
          关闭
        </button>
        <button
          class="h-10 rounded-xl bg-gradient-to-r from-indigo-500 to-violet-500 text-sm font-medium text-white transition hover:shadow-md disabled:cursor-not-allowed disabled:opacity-60"
          @click="handleCast"
          :disabled="!selectedDevice || !serverUrl || isCasting"
        >
          {{ isCasting ? '投屏中...' : '开始投屏' }}
        </button>
      </div>
    </div>
  </div>
</template>
