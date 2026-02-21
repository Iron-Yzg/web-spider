<script setup lang="ts">
import { ref, onMounted } from 'vue'
import type { LocalVideo } from '../types'
import type { DlnaDevice } from '../services/api'
import { 
  discoverDlnaDevices, 
  getLocalIpAddress,
  startDlnaMediaServer,
  stopDlnaMediaServer,
  castToDlnaDevice 
} from '../services/api'

const props = defineProps<{
  video: LocalVideo
}>()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const isDiscovering = ref(false)
const devices = ref<DlnaDevice[]>([])
const localIp = ref('')
const serverUrl = ref('')
const isStartingServer = ref(false)
const isCasting = ref(false)
const statusMessage = ref('')
const selectedDevice = ref<string | null>(null)

onMounted(async () => {
  await handleStartServer()
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
      discoverDlnaDevices(5)
    ])
    localIp.value = ip
    devices.value = foundDevices
    
    if (foundDevices.length === 0) {
      statusMessage.value = '未发现任何 DLNA 设备'
    } else {
      statusMessage.value = `发现 ${foundDevices.length} 个设备`
    }
  } catch (e) {
    statusMessage.value = `搜索失败: ${e}`
  } finally {
    isDiscovering.value = false
  }
}

async function handleStartServer() {
  if (!props.video) return
  
  isStartingServer.value = true
  statusMessage.value = '正在启动媒体服务器...'
  
  try {
    const url = await startDlnaMediaServer(props.video.file_path, 8080)
    serverUrl.value = url
    statusMessage.value = `服务器已启动: ${url}`
  } catch (e) {
    statusMessage.value = `启动服务器失败: ${e}`
  } finally {
    isStartingServer.value = false
  }
}

async function handleCast() {
  if (!selectedDevice.value && !serverUrl.value) return
  
  // 如果没有选择设备但有服务器URL，仍然启动服务器
  if (!selectedDevice.value && serverUrl.value) {
    statusMessage.value = '请选择投屏设备'
    return
  }
  
  isCasting.value = true
  statusMessage.value = '正在投屏...'
  
  try {
    if (selectedDevice.value && serverUrl.value) {
      await castToDlnaDevice(
        selectedDevice.value,
        serverUrl.value,
        props.video.name
      )
      statusMessage.value = '已发送播放命令，请查看电视。如果电视没有响应，请在电视浏览器中打开: ' + serverUrl.value
    } else if (serverUrl.value) {
      // 没有DLNA设备时，显示服务器URL
      statusMessage.value = '服务器已启动，请在电视浏览器中打开: ' + serverUrl.value
    }
  } catch (e) {
    // 失败时也显示URL，让用户可以手动在电视打开
    if (serverUrl.value) {
      statusMessage.value = '投屏失败，请在电视浏览器中打开: ' + serverUrl.value
    } else {
      statusMessage.value = `投屏失败: ${e}`
    }
  } finally {
    isCasting.value = false
  }
}

async function handleStopServer() {
  try {
    await stopDlnaMediaServer()
    serverUrl.value = ''
    statusMessage.value = '服务器已停止'
  } catch (e) {
    statusMessage.value = `停止服务器失败: ${e}`
  }
}

async function handleClose() {
  if (serverUrl.value) {
    try {
      await stopDlnaMediaServer()
    } catch (e) {
      console.error('停止服务器失败:', e)
    }
  }
  emit('close')
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
            {{ isDiscovering ? '搜索中...' : '搜索设备' }}
          </button>
          
          <button 
            v-if="!serverUrl"
            @click="handleStartServer" 
            :disabled="isStartingServer || !video"
            class="action-btn primary"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polygon points="5 3 19 12 5 21 5 3"></polygon>
            </svg>
            {{ isStartingServer ? '启动中...' : '启动服务器' }}
          </button>
          
          <button 
            v-else
            @click="handleStopServer" 
            class="action-btn danger"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="6" y="6" width="12" height="12"></rect>
            </svg>
            停止服务器
          </button>
        </div>
        
        <div v-if="serverUrl" class="server-info">
          <span class="label">服务器地址:</span>
          <code>{{ serverUrl }}</code>
        </div>
        
        <div v-if="devices.length > 0" class="device-list">
          <span class="label">选择投屏设备:</span>
          <div class="devices">
            <button 
              v-for="device in devices" 
              :key="device.udn"
              :class="['device-item', { selected: selectedDevice === device.name }]"
              @click="selectedDevice = device.name"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="2" y="7" width="20" height="15" rx="2" ry="2"></rect>
                <polyline points="17 2 12 7 7 2"></polyline>
              </svg>
              {{ device.name }}
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
  align-items: center;
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

.device-item:hover {
  border-color: #cbd5e1;
}

.device-item.selected {
  border-color: #667eea;
  background: #f0f5ff;
  color: #667eea;
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
