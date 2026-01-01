<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'

interface LogEntry {
  timestamp: string
  message: string
}

const props = defineProps<{
  visible: boolean
  title?: string
}>()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const logs = ref<LogEntry[]>([])
const isAutoScrolling = ref(true)

function addLog(message: string) {
  const now = new Date()
  const timestamp = now.toLocaleTimeString('zh-CN', { hour12: false })
  logs.value.push({ timestamp, message })

  // 自动滚动到底部
  if (isAutoScrolling.value) {
    nextTick(() => {
      const container = document.querySelector('.log-content')
      if (container) {
        container.scrollTop = container.scrollHeight
      }
    })
  }
}

// 暴露添加日志的方法给父组件
defineExpose({
  addLog
})

// 监听visible变化，重置日志
watch(() => props.visible, (newVal) => {
  if (newVal) {
    logs.value = []
  }
})

function clearLogs() {
  logs.value = []
}

function toggleAutoScroll() {
  isAutoScrolling.value = !isAutoScrolling.value
}

function handleClose() {
  emit('close')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="visible" class="log-overlay" @click.self="handleClose">
        <div class="log-window">
          <div class="log-header">
            <div class="log-title">
              <span class="title-icon">📋</span>
              <span>{{ title || '爬取日志' }}</span>
            </div>
            <div class="log-actions">
              <button @click="toggleAutoScroll" class="action-btn" :class="{ active: isAutoScrolling }">
                {{ isAutoScrolling ? '⏸️ 暂停滚动' : '▶️ 自动滚动' }}
              </button>
              <button @click="clearLogs" class="action-btn">🗑️ 清空</button>
              <button @click="handleClose" class="close-btn">✕</button>
            </div>
          </div>

          <div class="log-content">
            <div v-if="logs.length === 0" class="empty-logs">
              暂无日志输出...
            </div>
            <div v-for="(log, index) in logs" :key="index" class="log-entry">
              <span class="log-time">[{{ log.timestamp }}]</span>
              <span class="log-message">{{ log.message }}</span>
            </div>
          </div>

          <div class="log-footer">
            <span class="log-count">共 {{ logs.length }} 条日志</span>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.log-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(2px);
}

.log-window {
  width: 700px;
  max-width: 90vw;
  height: 500px;
  max-height: 80vh;
  background: #1a1a2e;
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
  overflow: hidden;
}

.log-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 14px 20px;
  background: linear-gradient(135deg, #1e1e3f, #16213e);
  border-bottom: 1px solid #2a2a4a;
}

.log-title {
  display: flex;
  align-items: center;
  gap: 10px;
  color: #fff;
  font-size: 15px;
  font-weight: 600;
}

.title-icon {
  font-size: 18px;
}

.log-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.action-btn {
  padding: 6px 12px;
  background: rgba(255, 255, 255, 0.1);
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 6px;
  color: #a0a0b0;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.15);
  color: #fff;
}

.action-btn.active {
  background: rgba(102, 126, 234, 0.3);
  border-color: #667eea;
  color: #667eea;
}

.close-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(239, 68, 68, 0.2);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 6px;
  color: #f87171;
  font-size: 14px;
  cursor: pointer;
  transition: all 0.2s;
}

.close-btn:hover {
  background: rgba(239, 68, 68, 0.4);
}

.log-content {
  flex: 1;
  padding: 12px 16px;
  overflow-y: auto;
  font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
  font-size: 12px;
  line-height: 1.6;
}

.empty-logs {
  color: #6b7280;
  text-align: center;
  padding: 40px 20px;
}

.log-entry {
  display: flex;
  gap: 10px;
  padding: 4px 0;
  color: #e0e0e0;
  animation: fadeIn 0.2s ease-out;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(-5px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.log-time {
  color: #6b7280;
  flex-shrink: 0;
  user-select: none;
}

.log-message {
  color: #e5e5e5;
  word-break: break-all;
}

.log-footer {
  padding: 10px 20px;
  background: #16162a;
  border-top: 1px solid #2a2a4a;
}

.log-count {
  color: #6b7280;
  font-size: 12px;
}

/* 动画 */
.modal-enter-active,
.modal-leave-active {
  transition: all 0.3s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from .log-window,
.modal-leave-to .log-window {
  transform: scale(0.95) translateY(20px);
  opacity: 0;
}
</style>
