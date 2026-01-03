<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { AppConfig, Website, LocalStorageItem } from '../types'

// 当前标签页
const activeTab = ref<'websites' | 'download'>('websites')

// 下载配置
const config = ref<AppConfig>({
  download_path: './downloads',
  local_storage: [],
  default_quality: 'auto'
})

// 网站列表
const websites = ref<Website[]>([])
const isLoading = ref(true)
const isSaving = ref(false)
const saveMessage = ref<{ type: 'success' | 'error', text: string } | null>(null)

let saveMessageTimeout: ReturnType<typeof setTimeout> | null = null

// 新建/编辑网站表单
const showWebsiteForm = ref(false)
const editingWebsite = ref<Website | null>(null)
const websiteForm = ref({
  name: '',
  base_url: '',
  local_storage: [] as LocalStorageItem[]
})
const newStorageKey = ref('')
const newStorageValue = ref('')

onMounted(async () => {
  await Promise.all([
    loadConfig(),
    loadWebsites()
  ])
  isLoading.value = false
})

async function loadConfig() {
  try {
    config.value = await invoke<AppConfig>('get_config')
  } catch (e) {
    console.error('加载配置失败:', e)
  }
}

async function loadWebsites() {
  try {
    websites.value = await invoke<Website[]>('get_websites')
  } catch (e) {
    console.error('加载网站列表失败:', e)
  }
}

function showSaveMessage(type: 'success' | 'error', text: string) {
  if (saveMessageTimeout) {
    clearTimeout(saveMessageTimeout)
  }
  saveMessage.value = { type, text }
  saveMessageTimeout = setTimeout(() => {
    saveMessage.value = null
  }, 3000)
}

// ========== 网站管理 ==========

function openWebsiteForm(website?: Website) {
  if (website) {
    editingWebsite.value = website
    websiteForm.value = {
      name: website.name,
      base_url: website.base_url,
      local_storage: [...website.local_storage]
    }
  } else {
    editingWebsite.value = null
    websiteForm.value = {
      name: '',
      base_url: '',
      local_storage: []
    }
  }
  newStorageKey.value = ''
  newStorageValue.value = ''
  showWebsiteForm.value = true
}

function closeWebsiteForm() {
  showWebsiteForm.value = false
  editingWebsite.value = null
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

  const exists = websiteForm.value.local_storage.some(item => item.key === key)
  if (exists) {
    alert('该 Key 已存在')
    return
  }

  websiteForm.value.local_storage.push({ key, value })
  newStorageKey.value = ''
  newStorageValue.value = ''
}

function removeStorageItem(index: number) {
  websiteForm.value.local_storage.splice(index, 1)
}

async function saveWebsite() {
  if (!websiteForm.value.name.trim()) {
    alert('请输入网站名称')
    return
  }
  if (!websiteForm.value.base_url.trim()) {
    alert('请输入网站地址')
    return
  }

  isSaving.value = true
  try {
    const website: Website = {
      id: editingWebsite.value?.id || crypto.randomUUID(),
      name: websiteForm.value.name.trim(),
      base_url: websiteForm.value.base_url.trim(),
      local_storage: websiteForm.value.local_storage,
      is_default: editingWebsite.value?.is_default || false
    }
    await invoke('save_website', { website })
    await loadWebsites()
    closeWebsiteForm()
    showSaveMessage('success', '✅ 网站已保存')
  } catch (e) {
    showSaveMessage('error', '❌ 保存失败: ' + e)
  } finally {
    isSaving.value = false
  }
}

async function deleteWebsite(id: string) {
  const name = websites.value.find(w => w.id === id)?.name || ''
  if (!confirm(`确定要删除网站 "${name}" 吗？`)) return

  try {
    await invoke('delete_website', { websiteId: id })
    await loadWebsites()
    showSaveMessage('success', '✅ 网站已删除')
  } catch (e) {
    showSaveMessage('error', '❌ 删除失败: ' + e)
  }
}

async function setDefaultWebsite(id: string) {
  try {
    await invoke('set_default_website', { websiteId: id })
    await loadWebsites()
    showSaveMessage('success', '✅ 已设为默认网站')
  } catch (e) {
    showSaveMessage('error', '❌ 设置失败: ' + e)
  }
}

// ========== 下载设置 ==========

async function saveDownloadConfig() {
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
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择下载目录'
    })
    if (selected) {
      if (Array.isArray(selected) && selected.length > 0) {
        config.value.download_path = selected[0]
      } else if (typeof selected === 'string') {
        config.value.download_path = selected
      }
    }
  } catch (e) {
    console.error('选择目录失败:', e)
    const manualPath = prompt('请输入下载目录路径:')
    if (manualPath && manualPath.trim()) {
      config.value.download_path = manualPath.trim()
    }
  }
}

async function checkFfmpeg() {
  try {
    const hasFfmpeg = await invoke<boolean>('check_ffmpeg')
    if (hasFfmpeg) {
      alert('✅ FFmpeg 已安装')
    } else {
      alert('❌ FFmpeg 未安装\n\n请运行: brew install ffmpeg')
    }
  } catch (e) {
    alert('检查失败: ' + e)
  }
}

const hasWebsites = computed(() => websites.value.length > 0)
const defaultWebsite = computed(() => websites.value.find(w => w.is_default))
</script>

<template>
  <div class="settings-page">
    <!-- 保存消息提示 -->
    <Transition name="fade">
      <div v-if="saveMessage" :class="['save-message', saveMessage.type]">
        {{ saveMessage.text }}
      </div>
    </Transition>

    <!-- 标签页导航 -->
    <div class="tabs-nav">
      <button
        :class="['tab-btn', { active: activeTab === 'websites' }]"
        @click="activeTab = 'websites'"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="10"/>
          <line x1="2" y1="12" x2="22" y2="12"/>
          <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
        </svg>
        网站管理
      </button>
      <button
        :class="['tab-btn', { active: activeTab === 'download' }]"
        @click="activeTab = 'download'"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
          <polyline points="7 10 12 15 17 10"/>
          <line x1="12" y1="15" x2="12" y2="3"/>
        </svg>
        下载设置
      </button>
    </div>

    <!-- 网站管理 -->
    <div v-if="activeTab === 'websites'" class="tab-content">
      <div v-if="!showWebsiteForm" class="websites-list">
        <div class="list-header">
          <h3>已配置网站</h3>
          <button @click="openWebsiteForm()" class="add-btn">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="12" y1="5" x2="12" y2="19"/>
              <line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            添加网站
          </button>
        </div>

        <div v-if="websites.length === 0" class="empty-state">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <circle cx="12" cy="12" r="10"/>
            <line x1="2" y1="12" x2="22" y2="12"/>
            <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10"/>
          </svg>
          <p>暂无配置网站</p>
          <p class="hint">点击上方按钮添加第一个网站</p>
        </div>

        <div v-else class="website-cards">
          <div v-for="website in websites" :key="website.id" class="website-card">
            <div class="card-header">
              <div class="card-title">
                <span class="name">{{ website.name }}</span>
                <span v-if="website.is_default" class="default-badge">默认</span>
              </div>
              <div class="card-actions">
                <button v-if="!website.is_default" @click="setDefaultWebsite(website.id)" class="action-btn" title="设为默认">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
                  </svg>
                </button>
                <button @click="openWebsiteForm(website)" class="action-btn" title="编辑">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                    <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                  </svg>
                </button>
                <button @click="deleteWebsite(website.id)" class="action-btn delete" title="删除">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline points="3 6 5 6 21 6"/>
                    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                  </svg>
                </button>
              </div>
            </div>
            <div class="card-body">
              <div class="url">{{ website.base_url }}</div>
              <div class="localstorage-info">
                <span class="count">{{ website.local_storage.length }} 个 LocalStorage 配置</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 网站表单 -->
      <div v-else class="website-form">
        <div class="form-header">
          <button @click="closeWebsiteForm" class="back-btn">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="19" y1="12" x2="5" y2="12"/>
              <polyline points="12 19 5 12 12 5"/>
            </svg>
            返回列表
          </button>
          <h3>{{ editingWebsite ? '编辑网站' : '添加网站' }}</h3>
        </div>

        <div class="form-content">
          <div class="form-group">
            <label>网站名称</label>
            <input
              type="text"
              v-model="websiteForm.name"
              placeholder="例如: 网站A"
              class="form-input"
            />
          </div>

          <div class="form-group">
            <label>网站地址</label>
            <input
              type="text"
              v-model="websiteForm.base_url"
              placeholder="例如: https://d1ibyof3mbdf0n.cloudfront.net/"
              class="form-input"
            />
            <span class="form-hint">爬取视频时使用的页面 URL</span>
          </div>

          <div class="form-group">
            <label>LocalStorage 配置</label>
            <span class="form-hint">在此网站控制台执行 localStorage 复制的内容</span>

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
            <div v-if="websiteForm.local_storage.length > 0" class="storage-table">
              <div class="table-header">
                <span class="col-key">Key</span>
                <span class="col-value">Value</span>
                <span class="col-action">操作</span>
              </div>
              <div class="table-body">
                <div
                  v-for="(item, index) in websiteForm.local_storage"
                  :key="index"
                  class="table-row"
                >
                  <span class="col-key">{{ item.key }}</span>
                  <span class="col-value" :title="item.value">{{ item.value }}</span>
                  <span class="col-action">
                    <button @click="removeStorageItem(index)" class="delete-btn" title="删除">
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <line x1="18" y1="6" x2="6" y2="18"/>
                        <line x1="6" y1="6" x2="18" y2="18"/>
                      </svg>
                    </button>
                  </span>
                </div>
              </div>
            </div>
            <div v-else class="empty-storage">
              暂无 LocalStorage 配置
            </div>
          </div>

          <div class="form-actions">
            <button @click="closeWebsiteForm" class="cancel-btn">取消</button>
            <button @click="saveWebsite" :disabled="isSaving" class="submit-btn">
              {{ isSaving ? '保存中...' : '保存网站' }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 下载设置 -->
    <div v-if="activeTab === 'download'" class="tab-content">
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
                选择
              </button>
            </div>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <span class="label-text">FFmpeg 状态</span>
              <span class="label-desc">用于视频下载的转码工具</span>
            </div>
            <div class="setting-control">
              <button @click="checkFfmpeg" class="check-btn">检查状态</button>
            </div>
          </div>
        </div>
      </div>

      <div class="save-section">
        <button @click="saveDownloadConfig" :disabled="isSaving" class="save-btn">
          {{ isSaving ? '保存中...' : '保存配置' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-page {
  height: 100%;
  overflow-y: auto;
  padding: 12px;
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

.fade-enter-active, .fade-leave-active {
  transition: all 0.3s ease;
}

.fade-enter-from, .fade-leave-to {
  opacity: 0;
  transform: translateX(20px);
}

/* 标签页导航 */
.tabs-nav {
  display: flex;
  gap: 8px;
  margin-bottom: 20px;
  background: #fff;
  padding: 8px;
  border-radius: 12px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 20px;
  background: transparent;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  color: #64748b;
  cursor: pointer;
  transition: all 0.2s;
}

.tab-btn:hover {
  background: #f1f5f9;
  color: #1a1a2e;
}

.tab-btn.active {
  background: linear-gradient(135deg, #667eea, #764ba2);
  color: white;
  box-shadow: 0 2px 8px rgba(102, 126, 234, 0.3);
}

/* 网站列表 */
.websites-list {
  width: 100%;
}

.list-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
  padding: 0 8px;
}

.list-header h3 {
  font-size: 18px;
  font-weight: 600;
  color: #1a1a2e;
}

.add-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 20px;
  background: linear-gradient(135deg, #667eea, #764ba2);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.add-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.3);
}

.empty-state {
  text-align: center;
  padding: 40px 20px;
  background: #fff;
  border-radius: 12px;
  color: #94a3b8;
  margin: 0 8px;
}

.website-cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 16px;
  padding: 0 8px;
}

.website-card {
  background: #fff;
  border-radius: 12px;
  padding: 16px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
  transition: all 0.2s;
}

.website-card:hover {
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 12px;
}

.card-title {
  display: flex;
  align-items: center;
  gap: 8px;
}

.card-title .name {
  font-size: 15px;
  font-weight: 600;
  color: #1a1a2e;
}

.default-badge {
  padding: 2px 8px;
  background: linear-gradient(135deg, #667eea15, #764ba215);
  color: #667eea;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
}

.card-actions {
  display: flex;
  gap: 4px;
}

.action-btn {
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

.action-btn:hover {
  background: #f1f5f9;
  color: #667eea;
}

.action-btn.delete:hover {
  background: #fee2e2;
  color: #dc2626;
}

.card-body {
  padding-top: 12px;
  border-top: 1px solid #f0f0f0;
}

.url {
  font-size: 12px;
  color: #64748b;
  margin-bottom: 8px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.localstorage-info {
  font-size: 12px;
  color: #94a3b8;
}

/* 网站表单 */
.website-form {
  width: 100%;
  background: #fff;
  border-radius: 12px;
}

.form-header {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 20px;
  border-bottom: 1px solid #f0f0f0;
}

.back-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  background: #f1f5f9;
  color: #64748b;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.back-btn:hover {
  background: #e2e8f0;
  color: #1a1a2e;
}

.form-header h3 {
  font-size: 16px;
  font-weight: 600;
}

.form-content {
  padding: 24px;
}

.form-group {
  margin-bottom: 24px;
}

.form-group label {
  display: block;
  font-size: 14px;
  font-weight: 500;
  color: #1a1a2e;
  margin-bottom: 8px;
}

.form-hint {
  display: block;
  font-size: 12px;
  color: #94a3b8;
  margin-top: 4px;
}

.form-input {
  width: 100%;
  padding: 10px 14px;
  border: 1px solid #e8e8e8;
  border-radius: 8px;
  font-size: 14px;
  transition: all 0.2s;
}

.form-input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.08);
}

/* LocalStorage 表单 */
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
}

.storage-input:focus {
  outline: none;
  border-color: #667eea;
}

.add-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 18px;
  background: #f1f5f9;
  color: #475569;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.add-btn:hover {
  background: #667eea;
  color: white;
  border-color: #667eea;
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
}

.table-body {
  max-height: 200px;
  overflow-y: auto;
}

.table-row {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  border-bottom: 1px solid #f5f5f5;
}

.table-row:last-child {
  border-bottom: none;
}

.col-key {
  width: 120px;
  flex-shrink: 0;
  font-size: 13px;
  font-weight: 500;
  color: #1a1a2e;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

.empty-storage {
  padding: 24px;
  text-align: center;
  color: #94a3b8;
  font-size: 13px;
  background: #fafbfc;
  border-radius: 8px;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding-top: 16px;
  border-top: 1px solid #f0f0f0;
}

.cancel-btn {
  padding: 10px 24px;
  background: #f1f5f9;
  color: #64748b;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  cursor: pointer;
  transition: all 0.2s;
}

.cancel-btn:hover {
  background: #e2e8f0;
}

.submit-btn {
  padding: 10px 24px;
  background: linear-gradient(135deg, #667eea, #764ba2);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.submit-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.3);
}

.submit-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* 下载设置 */
.settings-section {
  background: #fff;
  border-radius: 12px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
  overflow: hidden;
  width: 100%;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 20px 24px;
  border-bottom: 1px solid #f0f0f0;
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
}

.path-input:focus {
  outline: none;
  border-color: #667eea;
}

.browse-btn {
  padding: 10px 16px;
  background: #f1f5f9;
  color: #475569;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
}

.browse-btn:hover {
  background: #e2e8f0;
}

.check-btn {
  padding: 8px 14px;
  background: linear-gradient(135deg, #667eea08, #764ba208);
  color: #667eea;
  border: 1px solid #667eea30;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
}

.check-btn:hover {
  background: linear-gradient(135deg, #667eea, #764ba2);
  color: #fff;
  border-color: transparent;
}

.save-section {
  padding: 20px 0;
  display: flex;
  justify-content: flex-end;
}

.save-btn {
  padding: 12px 28px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 10px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
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
