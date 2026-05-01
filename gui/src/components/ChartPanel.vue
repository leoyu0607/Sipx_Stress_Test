<script setup lang="ts">
/**
 * ChartPanel.vue
 *
 * 修正 / 新增：
 * 1. 新增 ccr / mos 圖表 tab（對應 README §關鍵指標）
 * 2. 修正 y 軸 grid 顏色：light theme 下改用深色 grid line
 * 3. 修正 canvas 在 light theme 下文字顏色從 data-theme 屬性動態取得
 * 4. MOS tab 加入 1~5 固定 y 軸範圍及品質參考線（≥4.0/≥3.0）
 */
import { ref, watch, onMounted, onUnmounted, nextTick, inject } from 'vue'
import { useTestStore } from '../stores/testStore'

const store = useTestStore()
const isDark = inject<ReturnType<typeof ref<boolean>>>('isDark')!

const canvasRef  = ref<HTMLCanvasElement | null>(null)
const activeChart = ref<'cps' | 'conc' | 'asr' | 'ccr' | 'pdd' | 'fail' | 'mos'>('cps')

const CHART_COLORS: Record<typeof activeChart.value, string> = {
  cps:  '#00e5c0',
  conc: '#3d9fff',
  asr:  '#3d9fff',
  ccr:  '#00e5c0',
  pdd:  '#f5a623',
  fail: '#ff4d4d',
  mos:  '#a855f7',
}

const CHART_LABELS: Record<typeof activeChart.value, string> = {
  cps:  'throughput — CPS over time',
  conc: 'concurrency — active calls',
  asr:  'ASR — answer seizure ratio %',
  ccr:  'CCR — call completion rate %',
  pdd:  'PDD — post dial delay ms',
  fail: 'errors — failed calls',
  mos:  'MOS — mean opinion score (1~5)',
}

let dpr = 1

/** 從目前 theme 取得 canvas 繪圖顏色 */
function themeColors() {
  const dark = isDark.value
  return {
    grid:  dark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.06)',
    label: dark ? 'rgba(255,255,255,0.20)' : 'rgba(0,0,0,0.35)',
    empty: dark ? 'rgba(255,255,255,0.10)' : 'rgba(0,0,0,0.15)',
  }
}

function resize() {
  const canvas = canvasRef.value
  if (!canvas) return
  dpr = window.devicePixelRatio || 1
  const { width, height } = canvas.getBoundingClientRect()
  canvas.width  = width  * dpr
  canvas.height = height * dpr
  draw()
}

function draw() {
  const canvas = canvasRef.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')!
  const W   = canvas.width  / dpr
  const H   = canvas.height / dpr
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.scale(dpr, dpr)

  const { grid, label, empty } = themeColors()

  // 取得當前系列資料
  const key  = activeChart.value
  const data = key === 'mos'
    ? store.series.mos
    : store.series[key as keyof typeof store.series] as number[]

  if (data.length < 2) {
    ctx.fillStyle = empty
    ctx.font = '11px JetBrains Mono, monospace'
    ctx.textAlign = 'center'
    ctx.fillText(
      store.rtpMetrics.enabled === false && key === 'mos'
        ? '未啟用 RTP'
        : 'waiting for data...',
      W / 2, H / 2
    )
    ctx.setTransform(1,0,0,1,0,0)
    return
  }

  const pad = { l: 52, r: 14, t: 14, b: 28 }
  const cW  = W - pad.l - pad.r
  const cH  = H - pad.t - pad.b
  const color = CHART_COLORS[key]

  // MOS 用固定 y 軸 0~5；其他用 max * 1.15
  const isMos   = key === 'mos'
  const maxV    = isMos ? 5 : (Math.max(...data) * 1.15 || 1)
  const minV    = isMos ? 1 : 0
  const range   = maxV - minV
  const xStep   = cW / (data.length - 1)

  // Grid lines + y-axis labels
  ctx.font      = '10px JetBrains Mono, monospace'
  ctx.textAlign = 'right'
  const steps   = isMos ? 4 : 4
  for (let i = 0; i <= steps; i++) {
    const frac = i / steps
    const y    = pad.t + cH - frac * cH
    ctx.strokeStyle = grid
    ctx.lineWidth   = 0.5
    ctx.beginPath(); ctx.moveTo(pad.l, y); ctx.lineTo(pad.l + cW, y); ctx.stroke()

    const v = minV + frac * range
    const lbl =
      key === 'pdd'  ? `${v.toFixed(0)}ms`
      : key === 'asr' || key === 'ccr' ? `${v.toFixed(0)}%`
      : key === 'mos'  ? v.toFixed(1)
      : key === 'conc' ? `${Math.round(v)}`
      : v.toFixed(1)
    ctx.fillStyle = label
    ctx.fillText(lbl, pad.l - 5, y + 3)
  }

  // MOS 參考線 ≥4.0（優） / ≥3.0（普通）
  if (isMos) {
    const drawRefLine = (val: number, color: string, lbl: string) => {
      const y = pad.t + cH - ((val - minV) / range) * cH
      ctx.setLineDash([4, 3])
      ctx.strokeStyle = color
      ctx.lineWidth   = 0.8
      ctx.beginPath(); ctx.moveTo(pad.l, y); ctx.lineTo(pad.l + cW, y); ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle   = color
      ctx.textAlign   = 'left'
      ctx.fillText(lbl, pad.l + 2, y - 2)
    }
    drawRefLine(4.0, '#00e5c088', '4.0 優')
    drawRefLine(3.0, '#f5a62388', '3.0 普通')
    ctx.textAlign = 'right'
  }

  // 座標點
  const pts: [number, number][] = data.map((v, i) => [
    pad.l + i * xStep,
    pad.t + cH - Math.max(0, Math.min(1, (v - minV) / range)) * cH,
  ])

  // Fill area
  ctx.beginPath()
  ctx.moveTo(pts[0][0], pad.t + cH)
  for (const [x, y] of pts) ctx.lineTo(x, y)
  ctx.lineTo(pts[pts.length - 1][0], pad.t + cH)
  ctx.closePath()
  const grad = ctx.createLinearGradient(0, pad.t, 0, pad.t + cH)
  grad.addColorStop(0, color + '28')
  grad.addColorStop(1, color + '03')
  ctx.fillStyle = grad
  ctx.fill()

  // Line
  ctx.beginPath()
  ctx.moveTo(pts[0][0], pts[0][1])
  for (let i = 1; i < pts.length; i++) {
    const [px, py] = pts[i - 1]
    const [nx, ny] = pts[i]
    const cpx = (px + nx) / 2
    ctx.bezierCurveTo(cpx, py, cpx, ny, nx, ny)
  }
  ctx.strokeStyle = color
  ctx.lineWidth   = 1.5
  ctx.lineJoin    = 'round'
  ctx.stroke()

  // Latest dot + glow
  const [lx, ly] = pts[pts.length - 1]
  ctx.beginPath(); ctx.arc(lx, ly, 5, 0, Math.PI * 2)
  ctx.fillStyle = color + '30'; ctx.fill()
  ctx.beginPath(); ctx.arc(lx, ly, 3, 0, Math.PI * 2)
  ctx.fillStyle = color; ctx.fill()

  // X-axis ticks
  ctx.textAlign  = 'center'
  ctx.fillStyle  = label
  ctx.font       = '10px JetBrains Mono, monospace'
  const every    = Math.max(1, Math.floor(data.length / 6))
  for (let i = 0; i < data.length; i += every) {
    ctx.fillText(`${i}s`, pad.l + i * xStep, H - 5)
  }

  ctx.setTransform(1,0,0,1,0,0)
}

const TABS = ['cps','conc','asr','ccr','pdd','fail','mos'] as const

function switchChart(key: typeof activeChart.value) {
  activeChart.value = key
  nextTick(draw)
}

// 當系列長度或 theme 變化時重繪
watch([
  () => store.series[activeChart.value as keyof typeof store.series]?.length,
  () => store.series.mos.length,
  isDark,
  activeChart,
], () => nextTick(draw))

const ro = new ResizeObserver(() => { resize() })

onMounted(() => {
  if (canvasRef.value) {
    ro.observe(canvasRef.value.parentElement!)
    nextTick(resize)
  }
})
onUnmounted(() => ro.disconnect())
</script>

<template>
  <div class="chart-panel">
    <div class="panel-hdr">
      <span class="panel-title">{{ CHART_LABELS[activeChart] }}</span>

      <!-- RTP badge：若 RTP 啟用顯示 MOS 即時值 -->
      <div v-if="store.rtpMetrics.enabled" class="rtp-badge">
        <span class="rtp-dot" :class="store.mosRating"></span>
        MOS {{ store.rtpMetrics.mos.toFixed(2) }}
      </div>

      <div class="tabs">
        <button
          v-for="key in TABS"
          :key="key"
          class="tab"
          :class="{
            active: activeChart === key,
            'tab-rtp': key === 'mos',
            'tab-disabled': key === 'mos' && !store.rtpMetrics.enabled,
          }"
          :title="key === 'mos' && !store.rtpMetrics.enabled ? '啟用 --rtp 後顯示' : ''"
          @click="switchChart(key)"
        >{{ key.toUpperCase() }}</button>
      </div>
    </div>

    <div class="canvas-wrap">
      <canvas ref="canvasRef"></canvas>
    </div>
  </div>
</template>

<style scoped>
.chart-panel {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 10px 14px 10px;
  flex: 1;
}

.panel-hdr {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
  flex-shrink: 0;
}

.panel-title {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text1);
}

/* RTP MOS badge */
.rtp-badge {
  display: flex;
  align-items: center;
  gap: 5px;
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text1);
  background: var(--bg3);
  border: 1px solid var(--border2);
  padding: 2px 8px;
  border-radius: 99px;
}
.rtp-dot {
  width: 6px; height: 6px;
  border-radius: 50%;
  background: var(--text2);
}
.rtp-dot.excellent { background: #00e5c0; }
.rtp-dot.good      { background: #3d9fff; }
.rtp-dot.fair      { background: #f5a623; }
.rtp-dot.poor      { background: #ff4d4d; }

.tabs {
  display: flex;
  gap: 1px;
  margin-left: auto;
  background: var(--bg2);
  padding: 2px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
}

.tab {
  padding: 3px 9px;
  font-family: var(--mono);
  font-size: 10px;
  font-weight: 500;
  color: var(--text2);
  border-radius: 3px;
  cursor: pointer;
  border: none;
  background: transparent;
  transition: all 0.15s;
}
.tab:hover { color: var(--text1); }
.tab.active { background: var(--bg4); color: var(--text0); }

/* MOS tab — 紫色強調 */
.tab.tab-rtp { color: #a855f7; }
.tab.tab-rtp.active { background: rgba(168,85,247,0.15); color: #a855f7; }
.tab.tab-disabled { opacity: 0.35; cursor: not-allowed; }

.canvas-wrap {
  flex: 1;
  position: relative;
  min-height: 0;
}

canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
}
</style>
