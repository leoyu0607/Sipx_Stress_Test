<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useTestStore } from '../stores/testStore'

const store = useTestStore()
const canvasRef = ref<HTMLCanvasElement | null>(null)
const activeChart = ref<'cps' | 'conc' | 'asr' | 'pdd' | 'fail'>('cps')

const CHART_COLORS = {
  cps:  '#00e5c0',
  conc: '#3d9fff',
  asr:  '#3d9fff',
  pdd:  '#f5a623',
  fail: '#ff4d4d',
}
const CHART_LABELS = {
  cps:  'throughput — CPS over time',
  conc: 'concurrency — active calls',
  asr:  'ASR — answer seizure ratio %',
  pdd:  'PDD — post dial delay ms',
  fail: 'errors — failed calls',
}

let dpr = 1

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
  const W = canvas.width  / dpr
  const H = canvas.height / dpr
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.scale(dpr, dpr)

  const data = store.series[activeChart.value]

  if (data.length < 2) {
    ctx.fillStyle = 'rgba(255,255,255,0.1)'
    ctx.font = '11px JetBrains Mono, monospace'
    ctx.textAlign = 'center'
    ctx.fillText('waiting for data...', W / 2, H / 2)
    ctx.setTransform(1,0,0,1,0,0)
    return
  }

  const pad = { l: 46, r: 14, t: 12, b: 26 }
  const cW  = W - pad.l - pad.r
  const cH  = H - pad.t - pad.b
  const color  = CHART_COLORS[activeChart.value]
  const maxV   = Math.max(...data) * 1.15 || 1
  const xStep  = cW / (data.length - 1)

  // Grid lines + y-axis labels
  ctx.font = '10px JetBrains Mono, monospace'
  ctx.textAlign = 'right'
  for (let i = 0; i <= 4; i++) {
    const y = pad.t + cH - (i / 4) * cH
    ctx.strokeStyle = 'rgba(255,255,255,0.04)'
    ctx.lineWidth = 0.5
    ctx.beginPath()
    ctx.moveTo(pad.l, y)
    ctx.lineTo(pad.l + cW, y)
    ctx.stroke()

    const v = (i / 4) * maxV
    const label =
      activeChart.value === 'pdd'  ? `${v.toFixed(0)}ms`
      : activeChart.value === 'asr'  ? `${v.toFixed(0)}%`
      : activeChart.value === 'conc' ? `${Math.round(v)}`
      : v.toFixed(1)
    ctx.fillStyle = 'rgba(255,255,255,0.2)'
    ctx.fillText(label, pad.l - 5, y + 3)
  }

  // Compute points
  const pts: [number, number][] = data.map((v, i) => [
    pad.l + i * xStep,
    pad.t + cH - Math.max(0, Math.min(1, v / maxV)) * cH,
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
  ctx.lineWidth = 1.5
  ctx.lineJoin = 'round'
  ctx.stroke()

  // Latest dot + glow
  const [lx, ly] = pts[pts.length - 1]
  ctx.beginPath(); ctx.arc(lx, ly, 5, 0, Math.PI * 2)
  ctx.fillStyle = color + '30'; ctx.fill()
  ctx.beginPath(); ctx.arc(lx, ly, 3, 0, Math.PI * 2)
  ctx.fillStyle = color; ctx.fill()

  // X-axis ticks
  ctx.textAlign = 'center'
  ctx.fillStyle = 'rgba(255,255,255,0.15)'
  ctx.font = '10px JetBrains Mono, monospace'
  const every = Math.max(1, Math.floor(data.length / 6))
  for (let i = 0; i < data.length; i += every) {
    ctx.fillText(`${i}s`, pad.l + i * xStep, H - 5)
  }

  ctx.setTransform(1,0,0,1,0,0)
}

function switchChart(key: typeof activeChart.value) {
  activeChart.value = key
  nextTick(draw)
}

// Re-draw when series data changes
watch(() => store.series[activeChart.value].length, () => nextTick(draw))
watch(activeChart, () => nextTick(draw))

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
      <div class="tabs">
        <button
          v-for="key in (['cps','conc','asr','pdd','fail'] as const)"
          :key="key"
          class="tab"
          :class="{ active: activeChart === key }"
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
