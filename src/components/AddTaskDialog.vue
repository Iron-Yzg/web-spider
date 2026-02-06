<script setup lang="ts">
import { ref, watch } from 'vue'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'confirm', urls: string[]): void
}>()

const urlInput = ref('')
const urls = ref<string[]>([])
const errorMessage = ref('')

// 重置状态
watch(() => props.visible, (val) => {
  if (val) {
    urlInput.value = ''
    urls.value = []
    errorMessage.value = ''
  }
})

// 验证 URL
function isValidUrl(url: string): boolean {
  try {
    new URL(url)
    return true
  } catch {
    return false
  }
}

// 解析粘贴的多个 URL
function handlePaste(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text') || ''
  const lines = text.split(/[\n,]/).map(u => u.trim()).filter(u => u)
  for (const url of lines) {
    if (isValidUrl(url) && !urls.value.includes(url)) {
      urls.value.push(url)
    }
  }
}

// 手动添加 URL
function addUrl() {
  const url = urlInput.value.trim()
  if (!url) {
    errorMessage.value = '请输入视频链接'
    return
  }
  if (!isValidUrl(url)) {
    errorMessage.value = '链接格式不正确'
    return
  }
  if (urls.value.includes(url)) {
    errorMessage.value = '该链接已添加'
    return
  }
  urls.value.push(url)
  urlInput.value = ''
  errorMessage.value = ''
}

// 移除 URL
function removeUrl(index: number) {
  urls.value.splice(index, 1)
}

// 确认添加
function handleConfirm() {
  if (urls.value.length === 0) {
    errorMessage.value = '请添加至少一个视频链接'
    return
  }
  emit('confirm', [...urls.value])
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-fade">
      <div v-show="visible" class="dialog-overlay" @click.self="$emit('close')">
        <div class="dialog">
          <div class="dialog-header">
            <h3>添加下载任务</h3>
            <button @click="$emit('close')" class="close-btn">
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>

          <div class="dialog-body">
            <!-- 输入框 -->
            <div class="input-group">
              <input
                type="text"
                v-model="urlInput"
                @keyup.enter="addUrl"
                @paste="handlePaste"
                placeholder="输入视频链接后按回车添加"
                class="url-input"
              />
              <button @click="addUrl" class="add-btn-small">添加</button>
            </div>

            <!-- 错误提示 -->
            <div v-if="errorMessage" class="error-message">{{ errorMessage }}</div>

            <!-- URL 列表 -->
            <div v-if="urls.length > 0" class="url-list">
              <div class="list-header">
                <span>已添加 {{ urls.length }} 个链接</span>
                <button @click="urls = []" class="clear-all">清空</button>
              </div>
              <div class="list-body">
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
            </div>

            <!-- 提示 -->
            <div v-else class="url-tip">
              支持粘贴多个链接（换行或逗号分隔）
            </div>
          </div>

          <div class="dialog-footer">
            <button @click="$emit('close')" class="btn btn-secondary">取消</button>
            <button @click="handleConfirm" class="btn btn-primary" :disabled="urls.length === 0">
              开始添加 {{ urls.length > 0 ? `(${urls.length}个)` : '' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
}

.dialog {
  width: 90%;
  max-width: 560px;
  background: #fff;
  border-radius: 16px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.2);
  overflow: hidden;
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 20px 24px;
  border-bottom: 1px solid #f0f0f0;
}

.dialog-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #1a1a2e;
}

.close-btn {
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}

.close-btn:hover {
  color: #1a1a2e;
  background: #f5f6f8;
}

.dialog-body {
  padding: 24px;
}

.input-group {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

.url-input {
  flex: 1;
  padding: 12px 16px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  font-size: 14px;
  transition: all 0.2s;
}

.url-input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.add-btn-small {
  padding: 12px 20px;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  white-space: nowrap;
}

.add-btn-small:hover {
  background: #5a67d8;
}

.error-message {
  padding: 10px 14px;
  background: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: 8px;
  color: #dc2626;
  font-size: 13px;
  margin-bottom: 16px;
}

.url-list {
  border: 1px solid #f0f0f0;
  border-radius: 10px;
  overflow: hidden;
}

.list-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background: #fafbfc;
  border-bottom: 1px solid #f0f0f0;
  font-size: 13px;
  color: #64748b;
}

.clear-all {
  background: transparent;
  border: none;
  color: #667eea;
  font-size: 12px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  transition: all 0.2s;
}

.clear-all:hover {
  background: #eef2ff;
}

.list-body {
  max-height: 240px;
  overflow-y: auto;
}

.url-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 16px;
  border-bottom: 1px solid #f5f5f5;
}

.url-item:last-child {
  border-bottom: none;
}

.url-text {
  flex: 1;
  font-size: 12px;
  color: #64748b;
  word-break: break-all;
  margin-right: 12px;
}

.remove-url {
  background: transparent;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
  flex-shrink: 0;
}

.remove-url:hover {
  color: #dc2626;
  background: #fee2e2;
}

.url-tip {
  text-align: center;
  color: #94a3b8;
  font-size: 13px;
  padding: 20px;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 16px 24px;
  border-top: 1px solid #f0f0f0;
  background: #fafbfc;
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
  background: #f5f6f8;
  color: #64748b;
  border: 1px solid #e5e7eb;
}

.btn-secondary:hover {
  background: #e5e7eb;
  color: #1a1a2e;
}

.btn-primary {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
}

.btn-primary:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.35);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 过渡动画 */
.dialog-fade-enter-active,
.dialog-fade-leave-active {
  transition: all 0.3s ease;
}

.dialog-fade-enter-from,
.dialog-fade-leave-to {
  opacity: 0;
}

.dialog-fade-enter-from .dialog,
.dialog-fade-leave-to .dialog {
  transform: scale(0.95) translateY(10px);
}
</style>
