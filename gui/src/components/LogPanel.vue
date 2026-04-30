<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useTestStore } from '../stores/testStore'

const store = useTestStore()
const bodyRef = ref<HTMLDivElement | null>(null)
const autoScroll = ref(true)

watch(() => store.logs.length, async () => {
  if (!autoScroll.value) return
  await nextTick()
  if (bodyRef.value) bodyRef.value.scrollTop = bodyRef.value.scrollHeight
})

function onScroll() {
  if (!bodyRef.value) return
  const el = bodyRef.value
  autoScroll.value = el.scrollTop + el.clientHeight >= el.scrollHeight - 20
}

async function copyLog() {
  const text = store.logs
    .map(l => `${l.ts} [${l.level.toUpperCase()}] ${l.msg}`)
    .join('\n')
  await navigator.clipboard.writeText(text)
}

const levelColor: Record<string, string> = {
  ok:   'var(--accent)',
  info: 'var(--blue)',
  warn: 'var(--warn)',
  err:  'var(--danger)',
  dbg:  'var(--text2)',
}
</script>

<template>
  <div class="log-panel">
    <div class="log-hdr">
      <span class="log-title">event log</span>
      <span class="log-count">{{ store.logs.length }} events</span>
      <div class="log-actions">
        <button class="btn btn-sm"
                :class="{ active: autoScroll }"
                @click="autoScroll = !autoScroll"
                :title="autoScroll ? 'auto-scroll on' : 'auto-scroll off'">
          {{ autoScroll ? '↓ auto' : '⏸ paused' }}
        </button>
        <button class="btn btn-sm" @click="copyLog">copy</button>
        <button class="btn btn-sm" @click="store.clearLog()">clear</button>
      </div>
    </div>

    <div class="log-body" ref="bodyRef" @scroll="onScroll">
      <div
        v-for="(entry, i) in store.logs"
        :key="i"
        class="log-line"
      >
        <span class="log-ts">{{ entry.ts }}</span>
        <span class="log-level" :style="{ color: levelColor[entry.level] }">
          {{ entry.level.toUpperCase() }}
        </span>
        <span class="log-msg">{{ entry.msg }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.log-panel {
  border-top: 1px solid var(--border);
  height: 260px;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.log-hdr {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 14px;
  background: var(--bg1);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.log-title {
  font-family: var(--mono);
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.07em;
  color: var(--text1);
  text-transform: uppercase;
}

.log-count {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text2);
  background: var(--bg3);
  padding: 1px 6px;
  border-radius: 3px;
}

.log-actions {
  display: flex;
  gap: 4px;
  margin-left: auto;
}

.log-body {
  flex: 1;
  overflow-y: auto;
  padding: 3px 0;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.8;
}

.log-line {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 0 14px;
  transition: background 0.1s;
}
.log-line:hover { background: var(--bg1); }

.log-ts {
  color: var(--text3);
  min-width: 72px;
  flex-shrink: 0;
  font-size: 10px;
}

.log-level {
  min-width: 34px;
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 500;
}

.log-msg { color: var(--text1); }

/* auto-scroll active button style */
.btn.active {
  color: var(--accent);
  border-color: rgba(0,229,192,0.3);
  background: rgba(0,229,192,0.06);
}
</style>
