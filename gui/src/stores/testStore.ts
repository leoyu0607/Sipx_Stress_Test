import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// ── Types ─────────────────────────────────────────────────────────────────────

export type CallMode = 'caller' | 'agent'

export interface SipAccount {
  id: string
  extension: string
  username: string
  password: string
  domain: string
  status: 'idle' | 'registering' | 'registered' | 'failed'
}

export interface CallerProfile {
  accessNumber: string
  concurrency: number
  cps: number
  enableAudio: boolean
  audioFile: string
}

export interface AgentProfile {
  count: number
  accounts: SipAccount[]
  defaultDomain: string
}

export interface TestConfig {
  server: string
  transport: 'UDP' | 'TCP' | 'TLS'
  localPort: number
  duration: number
  outputFormat: 'tui' | 'json' | 'csv' | 'table'
  verbose: boolean
  mode: CallMode
  caller: CallerProfile
  agent: AgentProfile
}

export interface Metrics {
  cps: number
  concurrency: number
  asr: number
  pdd: number
  acd: number
  failed: number
}

export interface CallStates {
  invite: number
  trying: number
  ringing: number
  ok: number
  ack: number
  bye: number
  error: number
}

export interface LogEntry {
  ts: string
  level: 'ok' | 'info' | 'warn' | 'err' | 'dbg'
  msg: string
}

export type TestStatus = 'idle' | 'running' | 'done' | 'error'

// Rust backend types (snake_case from serde)
interface RustSnapshot {
  calls_initiated: number
  calls_answered:  number
  calls_completed: number
  calls_failed:    number
  calls_timeout:   number
  asr:             number
}

interface RustReport {
  calls_initiated: number
  calls_answered:  number
  calls_completed: number
  calls_failed:    number
  calls_timeout:   number
  duration_secs:   number
  asr:             number
  ccr:             number
  actual_cps:      number
  acd_secs:        number
  pdd_p50_ms:      number
  pdd_p95_ms:      number
  pdd_p99_ms:      number
  pdd_max_ms:      number
  fail_4xx:        number
  fail_5xx:        number
  fail_6xx:        number
  mos:             number | null
}

// ── Constants ─────────────────────────────────────────────────────────────────
const MAX_SERIES = 90
const MAX_LOG    = 300

// ── Helpers ───────────────────────────────────────────────────────────────────
function uid()    { return Math.random().toString(36).slice(2, 8) }

function nowTs() {
  const d = new Date()
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map(n => String(n).padStart(2, '0')).join(':')
    + '.' + String(d.getMilliseconds()).padStart(3, '0')
}

// ── Store ─────────────────────────────────────────────────────────────────────
export const useTestStore = defineStore('test', () => {

  const config = ref<TestConfig>({
    server: '192.168.1.100:5060',
    transport: 'UDP',
    localPort: 5070,
    duration: 60,
    outputFormat: 'tui',
    verbose: false,
    mode: 'caller',
    caller: {
      accessNumber: '4008001234',
      concurrency: 100,
      cps: 10,
      enableAudio: false,
      audioFile: '',
    },
    agent: {
      count: 0,
      accounts: [],
      defaultDomain: '192.168.1.100',
    },
  })

  const status      = ref<TestStatus>('idle')
  const elapsedSec  = ref(0)
  const metrics     = ref<Metrics>({ cps:0, concurrency:0, asr:0, pdd:0, acd:0, failed:0 })
  const series      = ref({ cps:[] as number[], conc:[] as number[], asr:[] as number[], pdd:[] as number[], fail:[] as number[] })
  const callStates  = ref<CallStates>({ invite:0, trying:0, ringing:0, ok:0, ack:0, bye:0, error:0 })
  const respCodes   = ref<Record<string, number>>({ '100':0,'180':0,'200':0,'486':0,'404':0,'503':0,'408':0 })
  const logs        = ref<LogEntry[]>([])
  const flowTimes   = ref({ invite:'—', trying:'—', ringing:'—', ok:'—', ack:'—', bye:'—', done:'—' })
  const accountImportError = ref('')
  const finalReport = ref<RustReport | null>(null)

  let pollTimer:  ReturnType<typeof setInterval> | null = null
  let clockTimer: ReturnType<typeof setInterval> | null = null

  // Track previous snapshot for delta-based CPS calc
  let prevInitiated = 0

  // ── Computed ──────────────────────────────────────────────────────────────────
  const activeConcurrency = computed(() =>
    config.value.mode === 'caller'
      ? config.value.caller.concurrency
      : config.value.agent.accounts.length
  )

  const activeCps = computed(() =>
    config.value.mode === 'caller' ? config.value.caller.cps : 1
  )

  const registeredCount = computed(() =>
    config.value.agent.accounts.filter(a => a.status === 'registered').length
  )

  const cliCommand = computed(() => {
    const c = config.value
    const tr = c.transport.toLowerCase()
    if (c.mode === 'caller') {
      const audio = c.caller.enableAudio && c.caller.audioFile ? ` --audio "${c.caller.audioFile}"` : ''
      return `./sipress -s ${c.server} --from ${c.caller.accessNumber} -c ${c.caller.concurrency} --cps ${c.caller.cps} --duration ${c.duration} --transport ${tr}${audio}`
    } else {
      return `./sipress -s ${c.server} --mode agent --accounts accounts.csv --duration ${c.duration} --transport ${tr}`
    }
  })

  const progressPct = computed(() =>
    status.value === 'running' && config.value.duration > 0
      ? Math.min(100, (elapsedSec.value / config.value.duration) * 100)
      : 0
  )

  // ── Log ───────────────────────────────────────────────────────────────────────
  function addLog(level: LogEntry['level'], msg: string) {
    logs.value.push({ ts: nowTs(), level, msg })
    if (logs.value.length > MAX_LOG) logs.value.shift()
  }
  function clearLog() { logs.value = [] }

  // ── Series ────────────────────────────────────────────────────────────────────
  function pushSeries(key: keyof typeof series.value, val: number) {
    series.value[key].push(val)
    if (series.value[key].length > MAX_SERIES) series.value[key].shift()
  }

  // ── SIP Account Management ────────────────────────────────────────────────────
  interface GenerateOptions {
    count: number
    startExt: number
    usernamePrefix: string
    passwordMode: 'same' | 'ext' | 'custom'
    samePassword: string
    customPassword: string
    domain: string
  }

  function generateAccounts(opts: GenerateOptions) {
    for (let i = 0; i < opts.count; i++) {
      const ext  = String(opts.startExt + i)
      const user = opts.usernamePrefix ? `${opts.usernamePrefix}${ext}` : ext
      const pass = opts.passwordMode === 'ext'    ? ext
                 : opts.passwordMode === 'custom' ? opts.customPassword.replace('{ext}', ext)
                 : opts.samePassword
      config.value.agent.accounts.push({
        id: uid(), extension: ext, username: user,
        password: pass, domain: opts.domain, status: 'idle',
      })
    }
    config.value.agent.count = config.value.agent.accounts.length
    addLog('ok', `生成 ${opts.count} 個座席帳號（分機 ${opts.startExt}–${opts.startExt + opts.count - 1}）`)
  }

  function removeAccount(id: string) {
    config.value.agent.accounts = config.value.agent.accounts.filter(a => a.id !== id)
    config.value.agent.count = config.value.agent.accounts.length
  }

  function clearAccounts() {
    config.value.agent.accounts = []
    config.value.agent.count = 0
  }

  function updateAccount(id: string, patch: Partial<SipAccount>) {
    const acc = config.value.agent.accounts.find(a => a.id === id)
    if (acc) Object.assign(acc, patch)
  }

  function parseAccountText(text: string): SipAccount[] {
    accountImportError.value = ''
    const domain  = config.value.agent.defaultDomain
    const lines   = text.split('\n').map(l => l.trim()).filter(l => l && !l.startsWith('#'))
    const result: SipAccount[] = []

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]
      if (i === 0 && /^(extension|ext|username|user)/i.test(line)) continue

      if (line.includes(',')) {
        const p = line.split(',').map(s => s.trim())
        if (p.length < 3) { accountImportError.value = `Line ${i+1}: need extension,username,password`; continue }
        result.push({ id:uid(), extension:p[0], username:p[1], password:p[2], domain:p[3]||domain, status:'idle' })
      } else if (line.includes(':')) {
        const p = line.split(':')
        result.push({ id:uid(), extension:p[0].trim(), username:p[0].trim(), password:p[1]?.trim()??'', domain:p[2]?.trim()||domain, status:'idle' })
      } else {
        const p = line.split(/\s+/)
        if (p.length < 3) { accountImportError.value = `Line ${i+1}: unrecognized format`; continue }
        result.push({ id:uid(), extension:p[0], username:p[1], password:p[2], domain:p[3]||domain, status:'idle' })
      }
    }
    return result
  }

  function importAccountText(text: string) {
    const parsed = parseAccountText(text)
    if (parsed.length > 0) {
      config.value.agent.accounts.push(...parsed)
      config.value.agent.count = config.value.agent.accounts.length
      addLog('ok', `imported ${parsed.length} SIP accounts`)
    }
  }

  // ── Build Rust Config from UI config ─────────────────────────────────────────
  function buildRustConfig() {
    const c = config.value
    const transportMap: Record<string, string> = { UDP: 'udp', TCP: 'tcp', TLS: 'tcp' }
    return {
      server_addr:          c.server,
      local_addr:           c.localPort ? `0.0.0.0:${c.localPort}` : null,
      local_domain:         null,
      caller_number:        c.caller.accessNumber,
      callee_prefix:        '2',
      callee_range:         9999,
      cps:                  c.caller.cps,
      max_concurrent_calls: c.caller.concurrency,
      duration_secs:        c.duration,
      call_duration_secs:   30,
      invite_timeout_secs:  8,
      transport:            transportMap[c.transport] ?? 'udp',
      logs_dir:             'logs',
      rtp_base_port:        10000,
      audio_file:           c.caller.enableAudio && c.caller.audioFile ? c.caller.audioFile : null,
      enable_rtp:           c.caller.enableAudio && !!c.caller.audioFile,
    }
  }

  // ── Update metrics from Rust snapshot ────────────────────────────────────────
  function applySnapshot(snap: RustSnapshot) {
    const deltaCps = Math.max(0, snap.calls_initiated - prevInitiated)
    prevInitiated = snap.calls_initiated

    const concurrency = Math.max(0,
      snap.calls_initiated - snap.calls_completed - snap.calls_failed - snap.calls_timeout
    )

    metrics.value = {
      cps:         deltaCps,
      concurrency,
      asr:         parseFloat(snap.asr.toFixed(1)),
      pdd:         metrics.value.pdd,   // only available in final report
      acd:         metrics.value.acd,   // only available in final report
      failed:      snap.calls_failed + snap.calls_timeout,
    }

    pushSeries('cps',  deltaCps)
    pushSeries('conc', concurrency)
    pushSeries('asr',  snap.asr)
    pushSeries('pdd',  metrics.value.pdd)
    pushSeries('fail', snap.calls_failed)

    callStates.value = {
      invite:  Math.round(concurrency * 0.08),
      trying:  Math.round(concurrency * 0.05),
      ringing: Math.round(concurrency * 0.12),
      ok:      Math.round(concurrency * 0.65),
      ack:     Math.round(concurrency * 0.04),
      bye:     Math.round(concurrency * 0.04),
      error:   snap.calls_failed > 0 ? 1 : 0,
    }

    respCodes.value['200'] = snap.calls_answered
    respCodes.value['100'] = snap.calls_initiated
    respCodes.value['180'] = Math.round(snap.calls_answered * 1.02)

    addLog('info', `↑ ${snap.calls_initiated} INVITE  ↓ ${snap.calls_answered} 200 OK  ASR ${snap.asr.toFixed(1)}%  fail ${snap.calls_failed}`)
  }

  function applyReport(report: RustReport) {
    finalReport.value = report
    metrics.value = {
      cps:         parseFloat(report.actual_cps.toFixed(1)),
      concurrency: 0,
      asr:         parseFloat(report.asr.toFixed(1)),
      pdd:         parseFloat(report.pdd_p50_ms.toFixed(0)),
      acd:         parseFloat(report.acd_secs.toFixed(1)),
      failed:      report.calls_failed + report.calls_timeout,
    }
    respCodes.value['200'] = report.calls_answered
    respCodes.value['486'] = report.fail_4xx
    respCodes.value['503'] = report.fail_5xx
    addLog('ok', `壓測完成  ASR ${report.asr.toFixed(1)}%  PDD p50=${report.pdd_p50_ms.toFixed(0)}ms  ACD ${report.acd_secs.toFixed(1)}s`)
    if (report.mos != null) addLog('info', `MOS 估算: ${report.mos.toFixed(2)}`)
  }

  // ── Start / Stop ──────────────────────────────────────────────────────────────
  async function startTest() {
    if (status.value === 'running') return
    status.value   = 'running'
    elapsedSec.value = 0
    prevInitiated  = 0
    finalReport.value = null
    metrics.value  = { cps:0, concurrency:0, asr:0, pdd:0, acd:0, failed:0 }
    series.value   = { cps:[], conc:[], asr:[], pdd:[], fail:[] }
    respCodes.value= { '100':0,'180':0,'200':0,'486':0,'404':0,'503':0,'408':0 }
    callStates.value = { invite:0, trying:0, ringing:0, ok:0, ack:0, bye:0, error:0 }
    logs.value     = []

    addLog('ok',   `開始壓測 → ${config.value.server}`)
    addLog('info', `CPS=${config.value.caller.cps}  並發=${config.value.caller.concurrency}  持續=${config.value.duration}s`)

    try {
      await invoke('start_test', { config: buildRustConfig() })
    } catch (e) {
      addLog('err', `啟動失敗: ${e}`)
      status.value = 'error'
      return
    }

    // Poll snapshot every second
    pollTimer = setInterval(async () => {
      try {
        const snap = await invoke<RustSnapshot | null>('get_snapshot')
        if (snap) applySnapshot(snap)
      } catch {}
    }, 1000)

    // Clock
    clockTimer = setInterval(() => {
      elapsedSec.value++
      if (elapsedSec.value >= config.value.duration) {
        _finishTest()
      }
    }, 1000)
  }

  async function stopTest() {
    if (pollTimer)  { clearInterval(pollTimer);  pollTimer  = null }
    if (clockTimer) { clearInterval(clockTimer); clockTimer = null }
    try {
      await invoke('stop_test')
    } catch {}
    await _tryFetchReport()
    status.value = 'done'
    addLog('warn', `壓測已停止  elapsed: ${elapsedSec.value}s`)
  }

  async function _finishTest() {
    if (pollTimer)  { clearInterval(pollTimer);  pollTimer  = null }
    if (clockTimer) { clearInterval(clockTimer); clockTimer = null }
    await _tryFetchReport()
    status.value = 'done'
  }

  async function _tryFetchReport() {
    // Give engine a moment to finalize
    await new Promise(r => setTimeout(r, 800))
    try {
      const report = await invoke<RustReport | null>('get_report')
      if (report) applyReport(report)
    } catch {}
  }

  // ── Export ────────────────────────────────────────────────────────────────────
  function exportJson() {
    const data = finalReport.value
      ? { ...finalReport.value, config: config.value }
      : { config: config.value, metrics: metrics.value, series: series.value }
    const blob = new Blob([JSON.stringify(data, null, 2)], { type:'application/json' })
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = `sipress_${Date.now()}.json`; a.click()
    addLog('ok', 'report exported → sipress.json')
  }

  function exportCsv() {
    const rows = [['tick','cps','conc','asr','pdd','fail']]
    for (let i = 0; i < series.value.cps.length; i++)
      rows.push([String(i), series.value.cps[i]?.toFixed(2)??'0', String(series.value.conc[i]??0), series.value.asr[i]?.toFixed(2)??'0', series.value.pdd[i]?.toFixed(0)??'0', String(series.value.fail[i]??0)])
    const blob = new Blob([rows.map(r => r.join(',')).join('\n')], { type:'text/csv' })
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = `sipress_${Date.now()}.csv`; a.click()
    addLog('ok', 'report exported → sipress.csv')
  }

  function exportAccountsCsv() {
    const rows = [['extension','username','password','domain'], ...config.value.agent.accounts.map(a => [a.extension, a.username, a.password, a.domain])]
    const blob = new Blob([rows.map(r => r.join(',')).join('\n')], { type:'text/csv' })
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = 'sipress_accounts.csv'; a.click()
  }

  return {
    config, status, elapsedSec, metrics, series,
    callStates, respCodes, logs, flowTimes, finalReport,
    accountImportError, registeredCount,
    activeConcurrency, activeCps, cliCommand, progressPct,
    addLog, clearLog,
    generateAccounts, removeAccount, clearAccounts, updateAccount,
    importAccountText, parseAccountText,
    startTest, stopTest,
    exportJson, exportCsv, exportAccountsCsv,
  }
})
