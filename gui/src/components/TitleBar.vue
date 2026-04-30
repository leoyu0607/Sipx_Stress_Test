<script setup lang="ts">
import { computed, inject, ref } from 'vue'
import { useTestStore } from '../stores/testStore'

const store = useTestStore()

// Theme (provided by App.vue)
const isDark      = inject<ReturnType<typeof ref<boolean>>>('isDark')!
const toggleTheme = inject<() => void>('toggleTheme')!

const timerDisplay = computed(() => {
  const s   = store.elapsedSec
  const m   = String(Math.floor(s / 60)).padStart(2, '0')
  const sec = String(s % 60).padStart(2, '0')
  return `${m}:${sec}`
})

const btnLabel = computed(() =>
  store.status === 'running' ? '■ stop test' : '▶ start test'
)
const btnClass = computed(() =>
  store.status === 'running' ? 'btn btn-danger' : 'btn btn-accent'
)

function toggleTest() {
  if (store.status === 'running') store.stopTest()
  else store.startTest()
}
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>

    <!-- Window controls -->
    <div class="wc-group">
      <div class="wc wc-close"></div>
      <div class="wc wc-min"></div>
      <div class="wc wc-max"></div>
    </div>

    <div class="logo">sip<em>ress</em></div>
    <div class="sep-v"></div>

    <!-- Status badge -->
    <div class="status-badge" :class="store.status">
      <div class="dot" :class="{ pulse: store.status === 'running' }"></div>
      <span>{{ store.status.toUpperCase() }}</span>
    </div>

    <!-- Target -->
    <div class="target-info" v-if="store.config.server">
      → <strong>{{ store.config.server }}</strong>
      <span class="transport-tag">{{ store.config.transport }}</span>
    </div>

    <!-- Progress bar -->
    <div class="progress-wrap" v-if="store.status === 'running'">
      <div class="progress-bar" :style="{ width: store.progressPct + '%' }"></div>
    </div>

    <!-- Timer -->
    <div class="timer">{{ timerDisplay }}</div>

    <!-- Actions -->
    <div class="actions">
      <button class="btn btn-sm"
              @click="store.exportJson()"
              :disabled="store.series.cps.length === 0">↓ JSON</button>
      <button class="btn btn-sm"
              @click="store.exportCsv()"
              :disabled="store.series.cps.length === 0">↓ CSV</button>

      <div class="sep-v"></div>

      <!-- Theme toggle -->
      <button class="btn btn-sm theme-btn" @click="toggleTheme" :title="isDark ? '切換亮色模式' : '切換暗色模式'">
        <span class="theme-icon">
          <!-- Sun (light mode icon, shown in dark mode) -->
          <svg v-if="isDark" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="5"/>
            <line x1="12" y1="1"  x2="12" y2="3"/>
            <line x1="12" y1="21" x2="12" y2="23"/>
            <line x1="4.22" y1="4.22"  x2="5.64" y2="5.64"/>
            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
            <line x1="1"  y1="12" x2="3"  y2="12"/>
            <line x1="21" y1="12" x2="23" y2="12"/>
            <line x1="4.22"  y1="19.78" x2="5.64"  y2="18.36"/>
            <line x1="18.36" y1="5.64"  x2="19.78" y2="4.22"/>
          </svg>
          <!-- Moon (dark mode icon, shown in light mode) -->
          <svg v-else width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
          </svg>
        </span>
        {{ isDark ? 'Light' : 'Dark' }}
      </button>

      <div class="sep-v"></div>

      <button :class="btnClass + ' btn-sm'" @click="toggleTest">
        {{ btnLabel }}
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 14px;
  height: 44px;
  background: var(--bg1);
  border-bottom: 1px solid var(--border);
  user-select: none;
  flex-shrink: 0;
  transition: background 0.25s, border-color 0.25s;
}

.wc-group { display: flex; gap: 6px; flex-shrink: 0; }
.wc {
  width: 12px; height: 12px;
  border-radius: 50%;
  cursor: pointer;
  transition: filter 0.15s;
}
.wc:hover { filter: brightness(1.4); }
.wc-close { background: #ff5f57; }
.wc-min   { background: #febc2e; }
.wc-max   { background: #28c840; }

.logo {
  font-family: var(--mono);
  font-size: 14px;
  font-weight: 700;
  color: var(--accent);
  letter-spacing: -0.5px;
}
.logo em { color: var(--text1); font-style: normal; font-weight: 300; }

.sep-v {
  width: 1px;
  height: 16px;
  background: var(--border2);
  flex-shrink: 0;
}

.status-badge {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border-radius: 99px;
  font-family: var(--mono);
  font-size: 10px;
  font-weight: 500;
  border: 1px solid;
  transition: all 0.3s;
}
.status-badge.idle    { color: var(--text1); border-color: var(--border2); }
.status-badge.running { color: var(--accent); border-color: rgba(0,184,154,0.35); background: rgba(0,184,154,0.08); }
.status-badge.done    { color: var(--blue);   border-color: rgba(26,127,212,0.35); background: rgba(26,127,212,0.08); }
.status-badge.error   { color: var(--danger); border-color: rgba(217,54,54,0.35);  background: rgba(217,54,54,0.08); }

.dot {
  width: 6px; height: 6px;
  border-radius: 50%;
  background: currentColor;
  flex-shrink: 0;
}
.dot.pulse { animation: pulse 1.5s ease-in-out infinite; }
@keyframes pulse {
  0%,100% { opacity: 1; transform: scale(1); }
  50%      { opacity: 0.3; transform: scale(0.8); }
}

.target-info {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text1);
}
.target-info strong { color: var(--text0); font-weight: 500; }
.transport-tag {
  margin-left: 4px;
  font-size: 10px;
  color: var(--text2);
  background: var(--bg3);
  padding: 1px 5px;
  border-radius: 3px;
}

.progress-wrap {
  flex: 1;
  max-width: 120px;
  height: 3px;
  background: var(--bg3);
  border-radius: 2px;
  overflow: hidden;
}
.progress-bar {
  height: 100%;
  background: var(--accent);
  border-radius: 2px;
  transition: width 1s linear;
}

.timer {
  font-family: var(--mono);
  font-size: 12px;
  font-weight: 500;
  color: var(--text0);
  background: var(--bg3);
  border: 1px solid var(--border2);
  padding: 3px 10px;
  border-radius: var(--radius);
  min-width: 56px;
  text-align: center;
  transition: background 0.25s, border-color 0.25s;
}

.actions {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
}

/* Theme button */
.theme-btn {
  display: flex;
  align-items: center;
  gap: 4px;
}
.theme-icon {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}
.theme-icon svg {
  transition: transform 0.3s ease;
}
.theme-btn:hover .theme-icon svg {
  transform: rotate(20deg);
}
</style>
