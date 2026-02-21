<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import type { LocalVideo } from '../types'
import type { CastDevice, CastProtocol } from '../services/api'
import { 
  discoverCastDevices,
  castMedia,
  stopCastPlayback,
  getLocalIpAddress,
  startDlnaMediaServer,
  stopDlnaMediaServer,
} from '../services/api'

const props = defineProps<{
  video: LocalVideo
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

onMounted(async () => {
  await handleStartServer()
  await handleDiscover()
})

watch(selectedProtocol, async () => {
  await handleDiscover()
})

async function handleDiscover() {
  isDiscovering.value = true
  statusMessage.value = '正在搜索设备...'
  devices.value = []
  selectedDevice.value = null
  
  try {
    const [ip, foundDevices] = await Promise.all([
      getLocalIpAddress(),
      discoverCastDevices(selectedProtocol.value, 5)
    ])
    localIp.value = ip
    devices.value = foundDevices
    
    if (foundDevices.length === 0) {
      statusMessage.value = '未发现可用设备'
    } else {
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

async function handleStartServer() {
  if (!props.video) return
  
  isStartingServer.value = true

  statusMessage.value = '正在启动媒体服务器...'
  
  try {
    const source = isLocalVideo.value ? props.video.file_path : (props.video.m3u8_url || '')
    if (!source) {
      throw new Error('未找到可投屏的视频地址')
    }
    const url = await startDlnaMediaServer(source, 8080)
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
    await castMedia(selectedProtocol.value, selectedDevice.value, serverUrl.value, props.video.name)
    statusMessage.value = '已发送播放命令，请查看电视'
  } catch (e) {
    statusMessage.value = `投屏失败: ${e}`
  } finally {
    isCasting.value = false
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
  <div class="dlna-overlay">
    <div class="dlna-dialog">
      <div class="dlna-header">
        <h3>一键投屏</h3>
        <button class="close-btn" @click="handleClose">
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
      
      <div class="dlna-content">
        <div class="video-info">
          <span class="label">投屏视频:</span>
          <span class="value">{{ video.name }}</span>
        </div>

        <div class="video-info">
          <span class="label">协议:</span>
          <select v-model="selectedProtocol" class="protocol-select">
            <option value="sony">Sony 优先（推荐）</option>
            <option value="auto">自动</option>
            <option value="dlna">DLNA</option>
            <option value="chromecast">Chromecast（实验）</option>
            <option value="airplay">AirPlay（实验）</option>
          </select>
        </div>
        
        <div class="action-row">
          <button 
            @click="handleDiscover" 
            :disabled="isDiscovering"
            class="action-btn"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10"></circle>
              <line x1="12" y1="8" x2="12" y2="12"></line>
              <line x1="12" y1="16" x2="12.01" y2="16"></line>
            </svg>
            {{ isDiscovering ? '搜索中...' : '刷新设备' }}
          </button>
        </div>
        
        <div v-if="serverUrl" class="server-info">
          <span class="label">{{ isLocalVideo ? '服务器地址:' : '视频地址:' }}</span>
          <code>{{ serverUrl }}</code>
        </div>
        
        <div v-if="devices.length > 0" class="device-list">
          <span class="label">选择投屏设备:</span>
          <div class="devices">
            <button 
              v-for="device in devices" 
              :key="device.id"
              :class="['device-item', { selected: selectedDevice === device.id, unavailable: !device.available }]"
              @click="device.available ? selectedDevice = device.id : null"
            >
              <span class="device-main">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="2" y="7" width="20" height="15" rx="2" ry="2"></rect>
                  <polyline points="17 2 12 7 7 2"></polyline>
                </svg>
                <span>{{ device.name }}</span>
              </span>
              <small class="proto">{{ protocolLabel(device.protocol) }}</small>
              <small v-if="device.note" class="note">{{ device.note }}</small>
            </button>
          </div>
        </div>
        
        <div v-if="statusMessage" :class="['status-message', { error: statusMessage.includes('失败') || statusMessage.includes('错误') }]">
          {{ statusMessage }}
        </div>
      </div>
      
      <div class="dlna-footer">
        <button class="cancel-btn" @click="handleClose">关闭</button>
        <button 
          class="cast-btn" 
          @click="handleCast"
          :disabled="!selectedDevice || !serverUrl || isCasting"
        >
          {{ isCasting ? '投屏中...' : '开始投屏' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dlna-overlay {
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

.dlna-dialog {
  background: white;
  border-radius: 12px;
  width: 90%;
  max-width: 480px;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.15);
}

.dlna-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid #f0f0f0;
}

.dlna-header h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: #1a1a2e;
}

.close-btn {
  background: none;
  border: none;
  padding: 4px;
  cursor: pointer;
  color: #64748b;
  border-radius: 4px;
}

.close-btn:hover {
  background: #f1f5f9;
  color: #1a1a2e;
}

.dlna-content {
  padding: 20px;
}

.video-info {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
  font-size: 14px;
}

.video-info .label {
  color: #64748b;
}

.video-info .value {
  color: #1a1a2e;
  font-weight: 500;
}

.protocol-select {
  border: 1px solid #e8e8e8;
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 13px;
  color: #1a1a2e;
  background: #fff;
}

.action-row {
  display: flex;
  gap: 12px;
  margin-bottom: 16px;
}

.action-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 10px 16px;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  background: white;
  font-size: 13px;
  font-weight: 500;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s;
}

.action-btn:hover:not(:disabled) {
  border-color: #cbd5e1;
  background: #f8fafc;
}

.action-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.action-btn.primary {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  border: none;
  color: white;
}

.action-btn.primary:hover:not(:disabled) {
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.action-btn.danger {
  background: #fee2e2;
  border: none;
  color: #dc2626;
}

.action-btn.danger:hover:not(:disabled) {
  background: #fecaca;
}

.server-info {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
  font-size: 13px;
}

.server-info .label {
  color: #64748b;
}

.server-info code {
  background: #f1f5f9;
  padding: 4px 8px;
  border-radius: 4px;
  font-family: monospace;
  font-size: 12px;
  color: #059669;
}

.device-list {
  margin-bottom: 16px;
}

.device-list .label {
  display: block;
  font-size: 13px;
  color: #64748b;
  margin-bottom: 8px;
}

.devices {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.device-item {
  display: flex;
  align-items: flex-start;
  flex-direction: column;
  gap: 6px;
  padding: 8px 12px;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  background: white;
  font-size: 13px;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s;
}

.device-main {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.device-item:hover {
  border-color: #cbd5e1;
}

.device-item.selected {
  border-color: #667eea;
  background: #f0f5ff;
  color: #667eea;
}

.device-item.unavailable {
  opacity: 0.65;
  cursor: not-allowed;
}

.proto {
  font-size: 11px;
  color: #6366f1;
}

.note {
  font-size: 11px;
  color: #94a3b8;
}

.status-message {
  padding: 10px;
  border-radius: 8px;
  background: #f0fdf4;
  color: #059669;
  font-size: 13px;
  text-align: center;
}

.status-message.error {
  background: #fef2f2;
  color: #dc2626;
}

.dlna-footer {
  display: flex;
  gap: 12px;
  padding: 16px 20px;
  border-top: 1px solid #f0f0f0;
}

.cancel-btn {
  flex: 1;
  padding: 10px;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  background: white;
  font-size: 14px;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s;
}

.cancel-btn:hover {
  background: #f8fafc;
}

.cast-btn {
  flex: 1;
  padding: 10px;
  border: none;
  border-radius: 8px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  font-size: 14px;
  font-weight: 500;
  color: white;
  cursor: pointer;
  transition: all 0.2s;
}

.cast-btn:hover:not(:disabled) {
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.cast-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
