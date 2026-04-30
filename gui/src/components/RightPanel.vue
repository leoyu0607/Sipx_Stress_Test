<script setup lang="ts">
import { computed } from 'vue'
import { useTestStore } from '../stores/testStore'

const store = useTestStore()

const stateMax = computed(() =>
  Math.max(...Object.values(store.callStates), 1)
)

const stateList = computed(() => [
  { key: 'invite',  label: 'INVITE',  color: 'var(--blue)',   val: store.callStates.invite },
  { key: 'trying',  label: '100 Try', color: 'var(--text1)',  val: store.callStates.trying },
  { key: 'ringing', label: '180 Ring',color: 'var(--warn)',   val: store.callStates.ringing },
  { key: 'ok',      label: '200 OK',  color: 'var(--accent)', val: store.callStates.ok },
  { key: 'ack',     label: 'ACK',     color: 'var(--accent)', val: store.callStates.ack },
  { key: 'bye',     label: 'BYE',     color: 'var(--text2)',  val: store.callStates.bye },
  { key: 'error',   label: 'Error',   color: 'var(--danger)', val: store.callStates.error },
])

const respList = computed(() =>
  Object.entries(store.respCodes)
    .filter(([, v]) => v > 0)
    .map(([code, cnt]) => ({
      code,
      cnt,
      color: code.startsWith('1') ? 'var(--text1)'
           : code.startsWith('2') ? 'var(--accent)'
           : 'var(--danger)',
    }))
)

const flowSteps = computed(() => [
  { dir: '→', label: 'INVITE',     color: 'var(--blue)',   time: store.flowTimes.invite, note: '' },
  { dir: '←', label: '100 Trying', color: 'var(--text1)',  time: store.flowTimes.trying, note: '' },
  { dir: '←', label: '180 Ringing',color: 'var(--warn)',   time: store.flowTimes.ringing, note: 'PDD start' },
  { dir: '←', label: '200 OK',     color: 'var(--accent)', time: store.flowTimes.ok, note: 'PDD end' },
  { dir: '→', label: 'ACK',        color: 'var(--accent)', time: store.flowTimes.ack, note: '' },
  { dir: '→', label: 'BYE',        color: 'var(--text1)',  time: store.flowTimes.bye, note: 'ACD end' },
  { dir: '←', label: '200 OK',     color: 'var(--text2)',  time: store.flowTimes.done, note: '' },
])
</script>

<template>
  <div class="right-panel">

    <!-- Call States -->
    <div class="rp-section">
      <div class="rp-title">call states</div>
      <div v-for="s in stateList" :key="s.key" class="state-row">
        <div class="state-name">{{ s.label }}</div>
        <div class="state-track">
          <div class="state-fill"
               :style="{ width: (s.val / stateMax * 100) + '%', background: s.color }">
          </div>
        </div>
        <div class="state-count">{{ s.val }}</div>
      </div>
    </div>

    <!-- Response Codes -->
    <div class="rp-section">
      <div class="rp-title">response codes</div>
      <div v-if="respList.length === 0" class="empty">—</div>
      <div v-for="r in respList" :key="r.code" class="rcode-row">
        <span class="rcode-label" :style="{ color: r.color }">{{ r.code }}</span>
        <span class="rcode-count">{{ r.cnt.toLocaleString() }}</span>
      </div>
    </div>

    <!-- SIP Flow -->
    <div class="rp-section flow-section">
      <div class="rp-title">last call flow</div>
      <div v-for="(step, i) in flowSteps" :key="i" class="flow-item">
        <div class="flow-line">
          <div class="flow-dot" :style="{ background: step.color }"></div>
          <div class="flow-msg" :style="{ color: step.color }">
            {{ step.dir === '→' ? '' : '' }}{{ step.label }} {{ step.dir }}
          </div>
          <div class="flow-time">{{ step.time }}</div>
        </div>
        <div class="flow-note" v-if="step.note">{{ step.note }}</div>
        <div class="flow-vline" v-if="i < flowSteps.length - 1"
             :style="{ borderColor: step.note ? 'var(--border3)' : 'var(--border)' }">
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
.right-panel {
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  width: 200px;
  flex-shrink: 0;
}

.rp-section {
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}

.rp-title {
  font-family: var(--mono);
  font-size: 9px;
  font-weight: 500;
  letter-spacing: 0.1em;
  color: var(--text2);
  text-transform: uppercase;
  margin-bottom: 8px;
}

/* State bars */
.state-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 5px;
}
.state-name {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text1);
  min-width: 48px;
}
.state-track {
  flex: 1;
  height: 3px;
  background: var(--bg3);
  border-radius: 2px;
  overflow: hidden;
}
.state-fill {
  height: 100%;
  border-radius: 2px;
  transition: width 0.6s ease;
}
.state-count {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text0);
  min-width: 24px;
  text-align: right;
}

/* Resp codes */
.rcode-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 3px;
}
.rcode-label {
  font-family: var(--mono);
  font-size: 11px;
  font-weight: 500;
}
.rcode-count {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text1);
}
.empty {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text2);
}

/* SIP Flow */
.flow-section { flex: 1; }
.flow-item { position: relative; }
.flow-line {
  display: flex;
  align-items: center;
  gap: 5px;
}
.flow-dot {
  width: 5px; height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
}
.flow-msg {
  font-family: var(--mono);
  font-size: 10px;
  flex: 1;
}
.flow-time {
  font-family: var(--mono);
  font-size: 9px;
  color: var(--text2);
}
.flow-note {
  font-size: 9px;
  color: var(--text2);
  margin-left: 10px;
  margin-bottom: 1px;
}
.flow-vline {
  width: 0;
  height: 8px;
  border-left: 1px dashed var(--border);
  margin-left: 2px;
}
</style>
