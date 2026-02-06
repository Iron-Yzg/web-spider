<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { AppConfig } from '../types'

const config = ref<AppConfig>({
  download_path: './downloads',
  local_storage: [],
  default_quality: 'auto'
})

const newStorageKey = ref('')
const newStorageValue = ref('')
const isLoading = ref(true)
const isSaving = ref(false)
const saveMessage = ref<{ type: 'success' | 'error', text: string } | null>(null)

let saveMessageTimeout: ReturnType<typeof setTimeout> | null = null

onMounted(async () => {
  try {
    config.value = await invoke<AppConfig>('get_config')
  } finally {
    isLoading.value = false
  }
})

function showSaveMessage(type: 'success' | 'error', text: string) {
  if (saveMessageTimeout) {
    clearTimeout(saveMessageTimeout)
  }
  saveMessage.value = { type, text }
  saveMessageTimeout = setTimeout(() => {
    saveMessage.value = null
  }, 3000)
}

async function saveConfig() {
  isSaving.value = true
  try {
    await invoke('update_config', { config: config.value })
    showSaveMessage('success', '✅ 配置已保存')
  } catch (e) {
    showSaveMessage('error', '❌ 保存失败: ' + e)
  } finally {
    isSaving.value = false
  }
}

async function selectDownloadPath() {
  try {
    // 使用 Tauri dialog 插件选择目录
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择下载目录'
    })
    // selected 可能是字符串或字符串数组
    if (selected) {
      if (Array.isArray(selected) && selected.length > 0) {
        config.value.download_path = selected[0]
      } else if (typeof selected === 'string') {
        config.value.download_path = selected
      }
    }
  } catch (e) {
    console.error('选择目录失败:', e)
    // 尝试手动输入
    const manualPath = prompt('请输入下载目录路径:')
    if (manualPath && manualPath.trim()) {
      config.value.download_path = manualPath.trim()
    }
  }
}

function addStorageItem() {
  const key = newStorageKey.value.trim()
  const value = newStorageValue.value.trim()

  if (!key) {
    alert('请输入 Key')
    return
  }
  if (!value) {
    alert('请输入 Value')
    return
  }

  // 检查是否已存在
  const exists = config.value.local_storage.some(item => item.key === key)
  if (exists) {
    alert('该 Key 已存在')
    return
  }

  config.value.local_storage.push({ key, value })
  newStorageKey.value = ''
  newStorageValue.value = ''
}

function removeStorageItem(index: number) {
  config.value.local_storage.splice(index, 1)
}

const localStorageList = computed(() => config.value.local_storage)
</script>

<template>
  <div class="config-page">
    <!-- 保存消息提示 -->
    <Transition name="fade">
      <div v-if="saveMessage" :class="['save-message', saveMessage.type]">
        {{ saveMessage.text }}
      </div>
    </Transition>

    <div class="config-container">
      <!-- 下载设置模块 -->
      <div class="settings-section">
        <div class="section-header">
          <div class="section-icon">📥</div>
          <div class="section-info">
            <h3>下载设置</h3>
            <p>配置视频下载相关的选项</p>
          </div>
        </div>
        <div class="section-content">
          <div class="setting-item">
            <div class="setting-label">
              <span class="label-text">下载目录</span>
              <span class="label-desc">视频文件保存的位置</span>
            </div>
            <div class="setting-control">
              <input
                type="text"
                v-model="config.download_path"
                placeholder="输入下载目录路径"
                class="path-input"
              />
              <button @click="selectDownloadPath" class="browse-btn">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                </svg>
                选择
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 网站设置模块 -->
      <div class="settings-section">
        <div class="section-header">
          <div class="section-icon">🌐</div>
          <div class="section-info">
            <h3>网站设置</h3>
            <p>配置爬取网站所需的认证信息</p>
          </div>
        </div>
        <div class="section-content">
          <div class="setting-item">
            <div class="setting-label">
              <span class="label-text">LocalStorage</span>
              <span class="label-desc">模拟登录状态的本地存储数据</span>
            </div>
          </div>

          <!-- 添加新条目 -->
          <div class="add-storage-form">
            <input
              type="text"
              v-model="newStorageKey"
              placeholder="Key"
              class="storage-input"
            />
            <input
              type="text"
              v-model="newStorageValue"
              placeholder="Value"
              class="storage-input"
            />
            <button @click="addStorageItem" class="add-btn">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="12" y1="5" x2="12" y2="19"/>
                <line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              添加
            </button>
          </div>

          <!-- 条目列表 -->
          <div class="storage-table">
            <div class="table-header">
              <span class="col-key">Key</span>
              <span class="col-value">Value</span>
              <span class="col-action">操作</span>
            </div>
            <div class="table-body">
              <div v-if="localStorageList.length === 0" class="empty-state">
                暂无配置，点击上方添加
              </div>
              <div
                v-for="(item, index) in localStorageList"
                :key="index"
                class="table-row"
              >
                <span class="col-key">{{ item.key }}</span>
                <span class="col-value" :title="item.value">{{ item.value }}</span>
                <span class="col-action">
                  <button @click="removeStorageItem(index)" class="delete-btn" title="删除">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="3 6 5 6 21 6"/>
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                    </svg>
                  </button>
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 保存按钮 -->
      <div class="save-section">
        <button @click="saveConfig" :disabled="isSaving" class="save-btn">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/>
            <polyline points="17 21 17 13 7 13 7 21"/>
            <polyline points="7 3 7 8 15 8"/>
          </svg>
          {{ isSaving ? '保存中...' : '保存配置' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.config-page {
  height: 100%;
  overflow-y: auto;
  padding: 20px;
  position: relative;
}

/* 保存消息提示 */
.save-message {
  position: fixed;
  top: 20px;
  right: 20px;
  padding: 12px 20px;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  z-index: 1000;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.save-message.success {
  background: #f0fdf4;
  color: #166534;
  border: 1px solid #bbf7d0;
}

.save-message.error {
  background: #fef2f2;
  color: #991b1b;
  border: 1px solid #fecaca;
}

.fade-enter-active,
.fade-leave-active {
  transition: all 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: translateX(20px);
}

.config-container {
  max-width: 800px;
  margin: 0 auto;
}

.settings-section {
  background: #fff;
  border-radius: 12px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04), 0 4px 12px rgba(0, 0, 0, 0.04);
  margin-bottom: 20px;
  overflow: hidden;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 20px 24px;
  border-bottom: 1px solid #f0f0f0;
  background: linear-gradient(to right, #fafbfc, #fff);
}

.section-icon {
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #667eea15, #764ba215);
  border-radius: 10px;
  font-size: 20px;
}

.section-info h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: #1a1a2e;
}

.section-info p {
  margin: 2px 0 0;
  font-size: 12px;
  color: #94a3b8;
}

.section-content {
  padding: 20px 24px;
}

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 0;
  border-bottom: 1px solid #f5f5f5;
}

.setting-item:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.setting-label {
  flex: 1;
}

.label-text {
  display: block;
  font-size: 14px;
  font-weight: 500;
  color: #1a1a2e;
}

.label-desc {
  display: block;
  font-size: 12px;
  color: #94a3b8;
  margin-top: 2px;
}

.setting-control {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.path-input {
  width: 280px;
  padding: 10px 14px;
  border: 1px solid #e8e8e8;
  border-radius: 8px;
  font-size: 13px;
  background: #fafbfc;
  transition: all 0.2s;
}

.path-input:focus {
  outline: none;
  border-color: #667eea;
  background: #fff;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.08);
}

.browse-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 16px;
  background: #f1f5f9;
  color: #475569;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.browse-btn:hover {
  background: #e2e8f0;
  border-color: #cbd5e1;
}

.check-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  background: linear-gradient(135deg, #667eea08, #764ba208);
  color: #667eea;
  border: 1px solid #667eea30;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.check-btn:hover {
  background: linear-gradient(135deg, #667eea, #764ba2);
  color: #fff;
  border-color: transparent;
}

/* LocalStorage */
.add-storage-form {
  display: flex;
  gap: 10px;
  margin-bottom: 16px;
}

.storage-input {
  flex: 1;
  padding: 10px 14px;
  border: 1px solid #e8e8e8;
  border-radius: 8px;
  font-size: 13px;
  transition: all 0.2s;
}

.storage-input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.08);
}

.add-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 18px;
  background: linear-gradient(135deg, #667eea, #764ba2);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.add-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.3);
}

/* 表格 */
.storage-table {
  border: 1px solid #f0f0f0;
  border-radius: 10px;
  overflow: hidden;
}

.table-header {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  background: #f8f9fa;
  border-bottom: 1px solid #f0f0f0;
  font-size: 12px;
  font-weight: 600;
  color: #64748b;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.table-body {
  max-height: 280px;
  overflow-y: auto;
}

.empty-state {
  padding: 32px 16px;
  text-align: center;
  color: #94a3b8;
  font-size: 13px;
}

.table-row {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  border-bottom: 1px solid #f5f5f5;
  transition: background 0.15s;
}

.table-row:last-child {
  border-bottom: none;
}

.table-row:hover {
  background: #fafbfc;
}

.col-key {
  width: 160px;
  flex-shrink: 0;
  font-size: 13px;
  font-weight: 500;
  color: #1a1a2e;
}

.col-value {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: #64748b;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  padding-right: 16px;
}

.col-action {
  width: 40px;
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
}

.delete-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: #94a3b8;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.delete-btn:hover {
  background: #fee2e2;
  color: #dc2626;
}

/* 保存按钮 */
.save-section {
  padding: 20px 0;
  display: flex;
  justify-content: flex-end;
}

.save-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 28px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 10px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.25);
}

.save-btn:hover:not(:disabled) {
  transform: translateY(-2px);
  box-shadow: 0 6px 20px rgba(102, 126, 234, 0.35);
}

.save-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
