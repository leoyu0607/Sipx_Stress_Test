<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useTestStore } from '../stores/testStore'
import GenerateDialog from './GenerateDialog.vue'

const store = useTestStore()
const copied = ref(false)
const fileInputRef = ref<HTMLInputElement | null>(null)
const dragOver = ref(false)
const showGenDialog = ref(false)

function handleGenConfirm(opts: Parameters<typeof store.generateAccounts>[0]) {
  store.generateAccounts(opts)
  showGenDialog.value = false
}

// Section collapse state
const sec = ref({ target:true, profile:true, output:true })
function toggleSec(k: keyof typeof sec.value) { sec.value[k] = !sec.value[k] }

// Copy CLI
async function copyCli() {
  await navigator.clipboard.writeText(store.cliCommand)
  copied.value = true
  setTimeout(() => (copied.value = false), 2000)
}

async function pickAudioFile() {
  const path = await open({ multiple: false, filters: [{ name: 'Audio', extensions: ['wav', 'pcm', 'raw', 'mp3'] }] })
  if (path && typeof path === 'string') store.config.caller.audioFile = path
}

// Account file upload
function triggerFileInput() { fileInputRef.value?.click() }

function onFileChange(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  readFile(file)
  ;(e.target as HTMLInputElement).value = ''
}

function onDrop(e: DragEvent) {
  dragOver.value = false
  e.preventDefault()
  const file = e.dataTransfer?.files[0]
  if (file) readFile(file)
}

function readFile(file: File) {
  const reader = new FileReader()
  reader.onload = ev => {
    const text = ev.target?.result as string
    store.importAccountText(text)
  }
  reader.readAsText(file)
}

// Generate blank accounts
// Status badge color
function statusColor(s: string) {
  return s === 'registered' ? 'var(--accent)'
       : s === 'registering' ? 'var(--warn)'
       : s === 'failed' ? 'var(--danger)'
       : 'var(--text2)'
}
function statusLabel(s: string) {
  return s === 'registered' ? 'REG' : s === 'registering' ? '...' : s === 'failed' ? 'ERR' : '—'
}


</script>

<template>
  <aside class="sidebar">

    <!-- ── TARGET ─────────────────────────────────────────── -->
    <div class="section">
      <div class="section-hdr" @click="toggleSec('target')">
        <span class="section-label">target</span>
        <span class="arrow" :class="{ open: sec.target }">▶</span>
      </div>
      <div class="section-body" v-show="sec.target">
        <div class="field">
          <label>SIP server <span class="tag">-s</span></label>
          <input v-model="store.config.server" type="text" placeholder="host:port">
        </div>
        <div class="field-row">
          <div class="field">
            <label>transport</label>
            <select v-model="store.config.transport">
              <option>UDP</option><option>TCP</option><option>TLS</option>
            </select>
          </div>
          <div class="field">
            <label>local port</label>
            <input v-model.number="store.config.localPort" type="number" min="1024" max="65535">
          </div>
        </div>
        <div class="field">
          <label>持續時間 (s) <span class="tag">--duration</span></label>
          <div class="slider-row">
            <input type="range" min="0" max="3600" v-model.number="store.config.duration">
            <input type="number" min="0" v-model.number="store.config.duration" class="num-input">
          </div>
          <div class="field-hint">{{ store.config.duration === 0 ? '0 = 不限時間（需搭配總通數或手動停止）' : `${store.config.duration} 秒後自動停止` }}</div>
        </div>
      </div>
    </div>

    <!-- ── MODE TABS ──────────────────────────────────────── -->
    <div class="mode-tabs">
      <button
        class="mode-tab"
        :class="{ active: store.config.mode === 'caller' }"
        @click="store.config.mode = 'caller'"
      >
        <span class="mode-icon">📞</span>
        <span class="mode-name">民眾端</span>
        <span class="mode-sub">主動撥出</span>
      </button>
      <button
        class="mode-tab"
        :class="{ active: store.config.mode === 'agent' }"
        @click="store.config.mode = 'agent'"
      >
        <span class="mode-icon">🎧</span>
        <span class="mode-name">座席端</span>
        <span class="mode-sub">SIP 話機</span>
      </button>
    </div>

    <!-- ── CALLER PROFILE ─────────────────────────────────── -->
    <div class="section" v-if="store.config.mode === 'caller'">
      <div class="section-hdr" @click="toggleSec('profile')">
        <span class="section-label">民眾端設定</span>
        <span class="arrow" :class="{ open: sec.profile }">▶</span>
      </div>
      <div class="section-body" v-show="sec.profile">

        <div class="field">
          <label>交換機接入號（主叫）</label>
          <input v-model="store.config.caller.accessNumber" type="text" placeholder="例：4008001234">
          <div class="field-hint">From 標頭中的號碼，告訴交換機「這通是誰打的」</div>
        </div>

        <div class="field">
          <label>被叫號碼 <span class="tag">--to</span></label>
          <input v-model="store.config.caller.calleeFixed" type="text" placeholder="例：8001（留空 = 隨機）">
          <div class="field-hint">{{ store.config.caller.calleeFixed ? `所有通話都打給 ${store.config.caller.calleeFixed}` : '留空時使用前綴+隨機尾數（壓測分散負載用）' }}</div>
        </div>

        <div class="field" v-show="!store.config.caller.calleeFixed">
          <label>被叫前綴 / 尾數範圍 <span class="tag">--to-prefix / --to-range</span></label>
          <div class="slider-row" style="gap:6px">
            <input v-model="store.config.caller.calleePrefix" type="text" placeholder="2" style="flex:1">
            <input type="number" min="0" v-model.number="store.config.caller.calleeRange" class="num-input">
          </div>
          <div class="field-hint">例：前綴 80 + 尾數 99 → 隨機產生 80~8099</div>
        </div>

        <div class="field">
          <label>併發數量 <span class="tag">-c</span></label>
          <div class="slider-row">
            <input type="range" min="1" max="1000" v-model.number="store.config.caller.concurrency">
            <input type="number" min="1" max="10000" v-model.number="store.config.caller.concurrency" class="num-input">
          </div>
        </div>

        <div class="field">
          <label>呼叫頻率 CPS <span class="tag">--cps</span></label>
          <div class="slider-row">
            <input type="range" min="1" max="200" v-model.number="store.config.caller.cps">
            <input type="number" min="1" max="1000" v-model.number="store.config.caller.cps" class="num-input">
          </div>
        </div>

        <div class="field">
          <label>總測試通數 <span class="tag">--max-calls</span></label>
          <div class="slider-row">
            <input type="range" min="0" max="10000" step="100" v-model.number="store.config.caller.totalCalls">
            <input type="number" min="0" v-model.number="store.config.caller.totalCalls" class="num-input">
          </div>
          <div class="field-hint">{{ store.config.caller.totalCalls === 0 ? '0 = 不限（依測試時長）' : `達 ${store.config.caller.totalCalls} 通後自動停止` }}</div>
        </div>

        <!-- Audio -->
        <div class="field">
          <div class="toggle-row">
            <span>播放音檔</span>
            <label class="toggle">
              <input type="checkbox" v-model="store.config.caller.enableAudio">
              <span class="track"><span class="thumb"></span></span>
            </label>
          </div>
        </div>

        <div class="field" v-if="store.config.caller.enableAudio">
          <label>音檔路徑</label>
          <div class="file-pick-row">
            <input v-model="store.config.caller.audioFile" type="text" placeholder="audio.wav" class="file-path-input">
            <button class="btn btn-sm" @click="pickAudioFile">瀏覽</button>
          </div>
          <div class="field-hint">支援 WAV / MP3 / PCM（8kHz/16bit 建議）</div>
        </div>

      </div>
    </div>

    <!-- ── AGENT PROFILE ──────────────────────────────────── -->
    <div class="section agent-section" v-if="store.config.mode === 'agent'">
      <div class="section-hdr" @click="toggleSec('profile')">
        <span class="section-label">座席端設定</span>
        <div class="section-hdr-right">
          <span class="acc-count-badge">
            {{ store.config.agent.accounts.length }} 帳號
          </span>
          <span class="arrow" :class="{ open: sec.profile }">▶</span>
        </div>
      </div>
      <div class="section-body agent-body" v-show="sec.profile">

        <!-- Default domain -->
        <div class="field">
          <label>預設 SIP Domain</label>
          <input v-model="store.config.agent.defaultDomain" type="text" placeholder="192.168.1.100">
        </div>

        <!-- Upload -->
        <div
          class="drop-zone"
          :class="{ over: dragOver }"
          @dragover.prevent="dragOver = true"
          @dragleave="dragOver = false"
          @drop="onDrop"
          @click="triggerFileInput"
        >
          <div class="drop-icon">↑</div>
          <div class="drop-main">上傳帳號清單</div>
          <div class="drop-sub">CSV / TXT · 拖曳或點擊</div>
          <input ref="fileInputRef" type="file" accept=".csv,.txt" style="display:none" @change="onFileChange">
        </div>

        <!-- Format hint -->
        <div class="format-hint">
          <div class="hint-title">支援格式</div>
          <div class="hint-line"><span class="hint-code">extension,username,password[,domain]</span></div>
          <div class="hint-line"><span class="hint-code">username:password[:domain]</span></div>
          <div class="hint-line"><span class="hint-code">ext user pass [domain]</span></div>
          <div class="hint-line muted">首行若含標頭會自動跳過</div>
        </div>

        <!-- Import error -->
        <div class="import-error" v-if="store.accountImportError">
          ⚠ {{ store.accountImportError }}
        </div>

        <!-- Generate via dialog -->
        <button class="btn btn-sm btn-accent gen-btn" @click="showGenDialog = true">
          + 快速生成帳號
        </button>

        <!-- Account table -->
        <div class="acc-table-wrap" v-if="store.config.agent.accounts.length > 0">
          <div class="acc-table-hdr">
            <div class="acc-col ext">分機</div>
            <div class="acc-col user">帳號</div>
            <div class="acc-col pass">密碼</div>
            <div class="acc-col stat">狀態</div>
            <div class="acc-col del"></div>
          </div>
          <div class="acc-table-body">
            <div
              v-for="acc in store.config.agent.accounts"
              :key="acc.id"
              class="acc-row"
            >
              <div class="acc-col ext">
                <input
                  :value="acc.extension"
                  @input="store.updateAccount(acc.id, { extension: ($event.target as HTMLInputElement).value })"
                  class="acc-input"
                  placeholder="1001"
                >
              </div>
              <div class="acc-col user">
                <input
                  :value="acc.username"
                  @input="store.updateAccount(acc.id, { username: ($event.target as HTMLInputElement).value })"
                  class="acc-input"
                  placeholder="user"
                >
              </div>
              <div class="acc-col pass">
                <input
                  :value="acc.password"
                  @input="store.updateAccount(acc.id, { password: ($event.target as HTMLInputElement).value })"
                  class="acc-input pass-input"
                  type="password"
                  placeholder="••••"
                >
              </div>
              <div class="acc-col stat">
                <span class="status-dot" :style="{ color: statusColor(acc.status) }">
                  {{ statusLabel(acc.status) }}
                </span>
              </div>
              <div class="acc-col del">
                <button class="del-btn" @click="store.removeAccount(acc.id)">×</button>
              </div>
            </div>
          </div>
        </div>

        <!-- Account actions -->
        <div class="acc-actions" v-if="store.config.agent.accounts.length > 0">
          <div class="reg-summary">
            <span class="reg-ok">{{ store.registeredCount }}</span>
            <span class="reg-sep">/</span>
            <span>{{ store.config.agent.accounts.length }}</span>
            <span class="reg-label">已註冊</span>
          </div>
          <div style="display:flex;gap:4px">
            <button class="btn btn-sm" @click="store.registerAll()" title="重新對所有未註冊帳號發 REGISTER">重新註冊</button>
            <button class="btn btn-sm" @click="store.exportAccountsCsv()">匯出 CSV</button>
            <button class="btn btn-sm" style="color:var(--danger)" @click="store.clearAccounts()">清除全部</button>
          </div>
        </div>

        <!-- Empty state -->
        <div class="acc-empty" v-else>
          上傳帳號清單或使用快速生成新增座席帳號
        </div>

      </div>
    </div>

    <!-- ── OUTPUT ─────────────────────────────────────────── -->
    <div class="section">
      <div class="section-hdr" @click="toggleSec('output')">
        <span class="section-label">output</span>
        <span class="arrow" :class="{ open: sec.output }">▶</span>
      </div>
      <div class="section-body" v-show="sec.output">
        <div class="field">
          <label>report format</label>
          <select v-model="store.config.outputFormat">
            <option value="tui">TUI dashboard</option>
            <option value="json">JSON</option>
            <option value="csv">CSV</option>
            <option value="table">Table</option>
          </select>
        </div>
        <div class="toggle-row field">
          <span>verbose log</span>
          <label class="toggle">
            <input type="checkbox" v-model="store.config.verbose">
            <span class="track"><span class="thumb"></span></span>
          </label>
        </div>
      </div>
    </div>

    <!-- ── CLI Preview ────────────────────────────────────── -->
    <div class="cli-section">
      <div class="cli-label">CLI PREVIEW</div>
      <div class="cli-box" @click="copyCli" :title="copied ? 'copied!' : 'click to copy'">
        <span class="cli-text">{{ store.cliCommand }}</span>
        <span class="cli-copy">{{ copied ? '✓' : '⎘' }}</span>
      </div>
    </div>

<!-- Generate Dialog -->
<GenerateDialog
  :visible="showGenDialog"
  :default-domain="store.config.agent.defaultDomain"
  @confirm="handleGenConfirm"
  @cancel="showGenDialog = false"
/>

  </aside>
</template>

<style scoped>
.sidebar {
  background: var(--bg1);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overflow-x: hidden;
  width: 260px;
  flex-shrink: 0;
}

/* Section */
.section { border-bottom: 1px solid var(--border); }

.section-hdr {
  display: flex;
  align-items: center;
  padding: 9px 14px 7px;
  cursor: pointer;
  user-select: none;
}
.section-hdr:hover { background: var(--bg2); }

.section-label {
  flex: 1;
  font-size: 10px;
  font-family: var(--mono);
  font-weight: 500;
  letter-spacing: 0.08em;
  color: var(--text2);
  text-transform: uppercase;
}

.section-hdr-right {
  display: flex;
  align-items: center;
  gap: 6px;
}

.acc-count-badge {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--accent);
  background: rgba(0,229,192,0.1);
  padding: 1px 6px;
  border-radius: 3px;
}

.arrow {
  font-size: 8px;
  color: var(--text2);
  transition: transform 0.2s;
}
.arrow.open { transform: rotate(90deg); }

.section-body { padding: 2px 12px 10px; }

/* Mode tabs */
.mode-tabs {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
  background: var(--bg0);
}

.mode-tab {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 8px 6px;
  border-radius: var(--radius);
  border: 1px solid var(--border2);
  background: var(--bg2);
  cursor: pointer;
  transition: all 0.15s;
  font-family: var(--sans);
}
.mode-tab:hover { border-color: var(--border3); background: var(--bg3); }
.mode-tab.active {
  border-color: rgba(0,229,192,0.35);
  background: rgba(0,229,192,0.06);
}

.mode-icon { font-size: 16px; line-height: 1; }
.mode-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text0);
}
.mode-sub {
  font-size: 10px;
  color: var(--text2);
  font-family: var(--mono);
}
.mode-tab.active .mode-name { color: var(--accent); }

/* Fields */
.field { margin-bottom: 8px; }
.field label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text1);
  margin-bottom: 3px;
}
.tag {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text2);
  background: var(--bg3);
  padding: 1px 5px;
  border-radius: 3px;
}
.field-hint {
  font-size: 10px;
  color: var(--text2);
  margin-top: 3px;
  font-family: var(--mono);
}

.field input[type="text"],
.field input[type="number"],
.field select {
  width: 100%;
  background: var(--bg0);
  border: 1px solid var(--border2);
  border-radius: var(--radius-sm);
  padding: 5px 8px;
  color: var(--text0);
  font-family: var(--mono);
  font-size: 12px;
  outline: none;
  transition: border-color 0.15s;
}
.field input:hover, .field select:hover { border-color: var(--border3); }
.field input:focus, .field select:focus { border-color: rgba(0,229,192,0.4); }

.field select {
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23464c5e'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 8px center;
  padding-right: 24px;
  cursor: pointer;
}

.field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }

/* Slider */
.slider-row { display: flex; align-items: center; gap: 6px; }
.slider-row input[type="range"] {
  flex: 1; height: 3px; background: var(--bg3); border-radius: 2px;
  appearance: none; outline: none; cursor: pointer; border: none; padding: 0;
}
.slider-row input[type="range"]::-webkit-slider-thumb {
  appearance: none; width: 13px; height: 13px; border-radius: 50%;
  background: var(--accent); border: 2px solid var(--bg1); cursor: pointer; transition: transform 0.1s;
}
.slider-row input[type="range"]::-webkit-slider-thumb:hover { transform: scale(1.2); }
.num-input { width: 58px !important; flex-shrink: 0; text-align: right; }

/* Toggle */
.toggle-row {
  display: flex; align-items: center;
  justify-content: space-between;
  font-size: 11px; color: var(--text1);
}
.toggle { position:relative; display:inline-block; width:30px; height:17px; cursor:pointer; }
.toggle input { opacity:0; width:0; height:0; position:absolute; }
.track { position:absolute; inset:0; background:var(--bg3); border:1px solid var(--border2); border-radius:9px; transition:all .2s; }
.thumb { position:absolute; top:2px; left:2px; width:11px; height:11px; background:var(--text2); border-radius:50%; transition:all .2s; }
.toggle input:checked ~ .track { background:rgba(0,229,192,.2); border-color:rgba(0,229,192,.4); }
.toggle input:checked ~ .track .thumb { transform:translateX(13px); background:var(--accent); }

/* Audio file pick */
.file-pick-row { display: flex; gap: 6px; }
.file-path-input { flex: 1 !important; }

/* Drop zone */
.drop-zone {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 12px 8px;
  border: 1px dashed var(--border2);
  border-radius: var(--radius);
  cursor: pointer;
  transition: all 0.15s;
  margin-bottom: 8px;
  text-align: center;
}
.drop-zone:hover, .drop-zone.over {
  border-color: rgba(0,229,192,0.4);
  background: rgba(0,229,192,0.04);
}
.drop-icon { font-size: 18px; color: var(--text2); line-height: 1; }
.drop-main { font-size: 12px; color: var(--text0); font-weight: 500; }
.drop-sub  { font-size: 10px; color: var(--text2); font-family: var(--mono); }

/* Format hint */
.format-hint {
  background: var(--bg0);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 6px 8px;
  margin-bottom: 8px;
}
.hint-title {
  font-size: 9px;
  font-family: var(--mono);
  color: var(--text2);
  text-transform: uppercase;
  letter-spacing: 0.07em;
  margin-bottom: 4px;
}
.hint-line { margin-bottom: 2px; }
.hint-code {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--accent);
}
.muted { font-family: var(--mono); font-size: 10px; color: var(--text2); }

/* Import error */
.import-error {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--danger);
  background: rgba(255,77,77,0.06);
  border: 1px solid rgba(255,77,77,0.2);
  border-radius: var(--radius-sm);
  padding: 5px 8px;
  margin-bottom: 8px;
}

/* Generate button */
.gen-btn { width: 100%; justify-content: center; margin-bottom: 8px; }

/* Account table */
.acc-table-wrap {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  overflow: hidden;
  margin-bottom: 6px;
  max-height: 180px;
  display: flex;
  flex-direction: column;
}

.acc-table-hdr {
  display: flex;
  gap: 0;
  background: var(--bg3);
  border-bottom: 1px solid var(--border);
  padding: 3px 6px;
  flex-shrink: 0;
}

.acc-table-body {
  overflow-y: auto;
  flex: 1;
}

.acc-row {
  display: flex;
  align-items: center;
  border-bottom: 1px solid var(--border);
  padding: 2px 6px;
  transition: background 0.1s;
}
.acc-row:last-child { border-bottom: none; }
.acc-row:hover { background: var(--bg2); }

.acc-col {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}
.acc-col.ext  { width: 44px; }
.acc-col.user { flex: 1; }
.acc-col.pass { width: 50px; }
.acc-col.stat { width: 26px; justify-content: center; }
.acc-col.del  { width: 18px; justify-content: center; }

/* header labels */
.acc-table-hdr .acc-col {
  font-family: var(--mono);
  font-size: 9px;
  color: var(--text2);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.acc-input {
  width: 100%;
  background: transparent;
  border: none;
  outline: none;
  padding: 2px 3px;
  color: var(--text0);
  font-family: var(--mono);
  font-size: 11px;
}
.acc-input:focus { background: var(--bg3); border-radius: 3px; }
.pass-input { color: var(--text1); letter-spacing: 0.1em; }

.status-dot {
  font-family: var(--mono);
  font-size: 9px;
  font-weight: 600;
}

.del-btn {
  background: none;
  border: none;
  color: var(--text2);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  padding: 0;
  transition: color 0.1s;
}
.del-btn:hover { color: var(--danger); }

/* Account actions */
.acc-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 4px;
}

.reg-summary {
  display: flex;
  align-items: center;
  gap: 3px;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text1);
}
.reg-ok  { color: var(--accent); font-weight: 600; }
.reg-sep { color: var(--text2); }
.reg-label { margin-left: 3px; color: var(--text2); font-size: 10px; }

.acc-empty {
  font-size: 11px;
  color: var(--text2);
  text-align: center;
  padding: 12px 0 4px;
  font-family: var(--mono);
  line-height: 1.6;
}

.agent-section { flex: 1; display: flex; flex-direction: column; }
.agent-body { flex: 1; overflow-y: auto; }

/* CLI */
.cli-section {
  margin-top: auto;
  padding: 10px 12px;
  border-top: 1px solid var(--border);
}
.cli-label {
  font-size: 10px; font-family: var(--mono); font-weight: 500;
  color: var(--text2); letter-spacing: 0.07em; margin-bottom: 5px;
}
.cli-box {
  display: flex; align-items: flex-start; gap: 6px;
  background: var(--bg0); border: 1px solid var(--border); border-radius: var(--radius-sm);
  padding: 6px 8px; cursor: pointer; transition: border-color 0.15s;
}
.cli-box:hover { border-color: var(--border3); }
.cli-text { font-family: var(--mono); font-size: 10px; color: var(--text1); word-break: break-all; line-height: 1.6; flex: 1; }
.cli-copy { font-size: 12px; color: var(--text2); flex-shrink: 0; margin-top: 1px; transition: color 0.2s; }
.cli-box:hover .cli-copy { color: var(--accent); }
</style>
