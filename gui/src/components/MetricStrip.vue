<script setup lang="ts">
import { computed } from 'vue'
import { useTestStore } from '../stores/testStore'

const store = useTestStore()

const isReady = computed(() => store.status !== 'idle')

const asrClass = computed(() => {
  const v = store.metrics.asr
  if (!isReady.value) return ''
  return v >= 85 ? 'c-accent' : v >= 70 ? 'c-warn' : 'c-danger'
})
</script>

<template>
  <div class="metric-strip">

    <div class="cell">
      <div class="name">CPS</div>
      <div class="val c-accent">{{ isReady ? store.metrics.cps.toFixed(1) : '—' }}</div>
      <div class="unit">calls / sec</div>
    </div>

    <div class="cell">
      <div class="name">CONCUR</div>
      <div class="val c-blue">{{ isReady ? store.metrics.concurrency : '—' }}</div>
      <div class="unit">active calls</div>
    </div>

    <div class="cell">
      <div class="name">ASR</div>
      <div class="val" :class="asrClass">
        {{ isReady ? store.metrics.asr.toFixed(1) + '%' : '—' }}
      </div>
      <div class="unit">answer rate</div>
    </div>

    <div class="cell">
      <div class="name">PDD</div>
      <div class="val c-warn">{{ isReady ? store.metrics.pdd + 'ms' : '—' }}</div>
      <div class="unit">avg delay</div>
    </div>

    <div class="cell">
      <div class="name">ACD</div>
      <div class="val">{{ isReady ? store.metrics.acd + 's' : '—' }}</div>
      <div class="unit">avg duration</div>
    </div>

    <div class="cell">
      <div class="name">FAILED</div>
      <div class="val c-danger">{{ isReady ? store.metrics.failed : '—' }}</div>
      <div class="unit">total errors</div>
    </div>

  </div>
</template>

<style scoped>
.metric-strip {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.cell {
  padding: 10px 14px;
  border-right: 1px solid var(--border);
  cursor: default;
  transition: background 0.15s;
}
.cell:last-child { border-right: none; }
.cell:hover { background: var(--bg1); }

.name {
  font-family: var(--mono);
  font-size: 9px;
  font-weight: 500;
  letter-spacing: 0.1em;
  color: var(--text2);
  text-transform: uppercase;
  margin-bottom: 3px;
}

.val {
  font-family: var(--mono);
  font-size: 22px;
  font-weight: 700;
  line-height: 1;
  color: var(--text0);
  transition: color 0.3s;
}
.val.c-accent { color: var(--accent); }
.val.c-blue   { color: var(--blue); }
.val.c-warn   { color: var(--warn); }
.val.c-danger { color: var(--danger); }

.unit {
  font-size: 10px;
  color: var(--text2);
  margin-top: 2px;
}
</style>
