<script setup lang="ts">
/**
 * RightPanel.vue
 *
 * 修正 / 新增：
 * 1. 新增「RTP 聲音品質」區塊（MOS / 掉包率 / Jitter / 封包統計）
 *    對應 README §RTP 聲音品質指標 與 HTML 報告 §RTP 聲音品質區塊
 * 2. MOS 顏色依 README 標準：≥4.0 優（accent）/ ≥3.0 普通（warn）/ <2.5 差（danger）
 * 3. 掉包率顏色：<1% 優 / <3% 普通 / ≥3% 差（依電話品質建議）
 * 4. Jitter 顏色：<30ms 優 / <60ms 普通 / ≥60ms 差
 */
import { computed } from 'vue'
import { useTestStore } from '../stores/testStore'

const store = useTestStore()

const stateMax = computed(() =>
  Math.max(...Object.values(store.callStates), 1)
)

const stateList = computed(() => [
  { key: 'invite',  label: 'INVITE',   color: 'var(--blue)',   val: store.callStates.invite },
  { key: 'trying',  label: '100 Try',  color: 'var(--text1)',  val: store.callStates.trying },
  { key: 'ringing', label: '180 Ring', color: 'var(--warn)',   val: store.callStates.ringing },
  { key: 'ok',      label: '200 OK',   color: 'var(--accent)', val: store.callStates.ok },
  { key: 'ack',     label: 'ACK',      color: 'var(--accent)', val: store.callStates.ack },
  { key: 'bye',     label: 'BYE',      color: 'var(--text2)',  val: store.callStates.bye },
  { key: 'error',   label: 'Error',    color: 'var(--danger)', val: store.callStates.error },
])

const respList = computed(() =>
  Object.entries(store.respCodes)
    .filter(([, v]) => v > 0)
    .map(([code, cnt]) => ({
      code, cnt,
      color: code.startsWith('1') ? 'var(--text1)'
           : code.startsWith('2') ? 'var(--accent)'
           : 'var(--danger)',
    }))
)

const flowSteps = computed(() => [
  { dir: '→', label: 'INVITE',      color: 'var(--blue)',   time: store.flowTimes.invite,  note: '' },
  { dir: '←', label: '100 Trying',  color: 'var(--text1)',  time: store.flowTimes.trying,  note: '' },
  { dir: '←', label: '180 Ringing', color: 'var(--warn)',   time: store.flowTimes.ringing, note: 'PDD start' },
  { dir: '←', label: '200 OK',      color: 'var(--accent)', time: store.flowTimes.ok,      note: 'PDD end' },
  { dir: '→', label: 'ACK',         color: 'var(--accent)', time: store.flowTimes.ack,     note: '' },
  { dir: '→', label: 'BYE',         color: 'var(--text1)',  time: store.flowTimes.bye,     note: 'ACD end' },
  { dir: '←', label: '200 OK',      color: 'var(--text2)',  time: store.flowTimes.done,    note: '' },
])

// ── RTP 指標顏色 ─────────────────────────────────────────────────

const rtp = computed(() => store.rtpMetrics)

// RTP 是否「已設定啟用」（不管是否完成，只要 enableAudio=true 且有音檔就顯示啟用中）
const rtpConfigured = computed(() =>
  store.config.caller.enableAudio && !!store.config.caller.audioFile
)

const mosColor = computed(() => {
  const m = rtp.value.mos
  if (m >= 4.0) return 'var(--accent)'
  if (m >= 3.0) return 'var(--warn)'
  return 'var(--danger)'
})

const mosLabel = computed(() => {
  const m = rtp.value.mos
  if (m >= 4.0) return '優'
  if (m >= 3.0) return '普通'
  if (m > 0)    return '差'
  return '—'
})

const lossColor = computed(() => {
  const p = rtp.value.packetLoss
  if (p < 1)  return 'var(--accent)'
  if (p < 3)  return 'var(--warn)'
  return 'var(--danger)'
})

const jitterColor = computed(() => {
  const j = rtp.value.jitter
  if (j < 30) return 'var(--accent)'
  if (j < 60) return 'var(--warn)'
  return 'var(--danger)'
})
</script>

<template>
  <div class="right-panel">

    <!-- ── Call States ─────────────────────────────────── -->
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

    <!-- ── RTP 聲音品質 ──────────────────────────────────── -->
    <div class="rp-section">
      <div class="rp-title">
        RTP 聲音品質
        <span v-if="rtp.enabled" class="rtp-on">G.711A</span>
        <span v-else-if="rtpConfigured && store.status === 'running'" class="rtp-active">啟用中</span>
        <span v-else-if="!rtpConfigured" class="rtp-off">未啟用</span>
      </div>

      <!-- 測試完成後顯示 MOS 等統計 -->
      <template v-if="rtp.enabled">
        <!-- MOS -->
        <div class="rtp-row">
          <div class="rtp-key">MOS</div>
          <div class="rtp-bar-wrap">
            <div class="rtp-bar"
                 :style="{ width: ((rtp.mos - 1) / 4 * 100) + '%', background: mosColor }">
            </div>
          </div>
          <div class="rtp-val" :style="{ color: mosColor }">
            {{ rtp.mos.toFixed(2) }}
            <span class="rtp-tag">{{ mosLabel }}</span>
          </div>
        </div>

        <!-- 掉包率 -->
        <div class="rtp-row">
          <div class="rtp-key">掉包</div>
          <div class="rtp-bar-wrap">
            <div class="rtp-bar"
                 :style="{ width: Math.min(rtp.packetLoss * 10, 100) + '%', background: lossColor }">
            </div>
          </div>
          <div class="rtp-val" :style="{ color: lossColor }">
            {{ rtp.packetLoss.toFixed(2) }}%
          </div>
        </div>

        <!-- Jitter -->
        <div class="rtp-row">
          <div class="rtp-key">Jitter</div>
          <div class="rtp-bar-wrap">
            <div class="rtp-bar"
                 :style="{ width: Math.min(rtp.jitter / 150 * 100, 100) + '%', background: jitterColor }">
            </div>
          </div>
          <div class="rtp-val" :style="{ color: jitterColor }">
            {{ rtp.jitter.toFixed(1) }}ms
          </div>
        </div>

        <!-- 封包統計 -->
        <div class="rtp-stats">
          <div class="rtp-stat-item">
            <span class="rtp-stat-key">Sent</span>
            <span class="rtp-stat-val">{{ rtp.packetsSent.toLocaleString() }}</span>
          </div>
          <div class="rtp-stat-item">
            <span class="rtp-stat-key">Recv</span>
            <span class="rtp-stat-val">{{ rtp.packetsRecv.toLocaleString() }}</span>
          </div>
          <div class="rtp-stat-item">
            <span class="rtp-stat-key">OOO</span>
            <span class="rtp-stat-val" :style="{ color: rtp.outOfOrder > 0 ? 'var(--warn)' : 'var(--text1)' }">
              {{ rtp.outOfOrder }}
            </span>
          </div>
        </div>
      </template>

      <!-- 測試執行中：已設定 RTP 但還沒有結果 -->
      <div v-else-if="rtpConfigured" class="rtp-hint">
        G.711A (PCMA) ← 傳送中<br>
        <span style="color:var(--text2);font-size:11px;">結果於測試結束後顯示</span>
      </div>

      <!-- RTP 未啟用提示 -->
      <div v-else class="rtp-hint">
        使用 <code>--rtp</code> 啟用<br>
        可選 <code>--audio &lt;file&gt;</code> 指定音檔
      </div>
    </div>

    <!-- ── Response Codes ──────────────────────────────── -->
    <div class="rp-section">
      <div class="rp-title">response codes</div>
      <div v-if="respList.length === 0" class="empty">—</div>
      <div v-for="r in respList" :key="r.code" class="rcode-row">
        <span class="rcode-label" :style="{ color: r.color }">{{ r.code }}</span>
        <span class="rcode-count">{{ r.cnt.toLocaleString() }}</span>
      </div>
    </div>

    <!-- ── SIP Flow ────────────────────────────────────── -->
    <div class="rp-section flow-section">
      <div class="rp-title">last call flow</div>
      <div v-for="(step, i) in flowSteps" :key="i" class="flow-item">
        <div class="flow-line">
          <div class="flow-dot" :style="{ background: step.color }"></div>
          <div class="flow-msg" :style="{ color: step.color }">
            {{ step.label }} {{ step.dir }}
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
  width: 210px;
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
  display: flex;
  align-items: center;
  gap: 6px;
}

.rtp-off {
  font-size: 9px;
  color: var(--text2);
  background: var(--bg3);
  border: 1px solid var(--border);
  padding: 1px 5px;
  border-radius: 3px;
  text-transform: lowercase;
}
.rtp-active {
  font-size: 9px;
  color: var(--warn);
  background: color-mix(in srgb, var(--warn) 15%, transparent);
  border: 1px solid var(--warn);
  padding: 1px 5px;
  border-radius: 3px;
}
.rtp-on {
  font-size: 9px;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  border: 1px solid var(--accent);
  padding: 1px 5px;
  border-radius: 3px;
}

/* ── State bars ─────────────────────────── */
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
  min-width: 52px;
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

/* ── RTP 品質 ───────────────────────────── */
.rtp-row {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-bottom: 6px;
}
.rtp-key {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text1);
  min-width: 36px;
}
.rtp-bar-wrap {
  flex: 1;
  height: 3px;
  background: var(--bg3);
  border-radius: 2px;
  overflow: hidden;
}
.rtp-bar {
  height: 100%;
  border-radius: 2px;
  transition: width 0.6s ease;
}
.rtp-val {
  font-family: var(--mono);
  font-size: 10px;
  min-width: 52px;
  text-align: right;
  display: flex;
  align-items: center;
  gap: 3px;
  justify-content: flex-end;
}
.rtp-tag {
  font-size: 9px;
  opacity: 0.75;
}

.rtp-stats {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}
.rtp-stat-item {
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.rtp-stat-key {
  font-family: var(--mono);
  font-size: 8px;
  color: var(--text2);
  text-transform: uppercase;
}
.rtp-stat-val {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text1);
}

.rtp-hint {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text2);
  line-height: 1.7;
}
.rtp-hint code {
  color: var(--accent);
  background: rgba(0,229,192,0.08);
  padding: 0 3px;
  border-radius: 2px;
}

/* ── Resp codes ─────────────────────────── */
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

/* ── SIP Flow ───────────────────────────── */
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
