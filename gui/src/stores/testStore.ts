import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

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
  ccr: number
  pdd: number
  acd: number
  failed: number
}

export interface RtpMetrics {
  enabled: boolean
  mos: number
  packetLoss: number
  jitter: number
  packetsSent: number
  packetsRecv: number
  outOfOrder: number
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

// ── Constants ─────────────────────────────────────────────────────────────────
const MAX_SERIES = 90
const MAX_LOG    = 300

// ── Helpers ───────────────────────────────────────────────────────────────────
function gauss(mean: number, sd: number): number {
  let u = 0, v = 0
  while (!u) u = Math.random()
  while (!v) v = Math.random()
  return mean + sd * Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v)
}
function clamp(v: number, lo: number, hi: number) { return Math.max(lo, Math.min(hi, v)) }
function ri(a: number, b: number) { return Math.floor(Math.random() * (b - a + 1)) + a }
function randId() { return Math.random().toString(16).slice(2, 10).toUpperCase() }
function randIp() { return `10.${ri(0,255)}.${ri(0,255)}.${ri(1,254)}` }
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
  const metrics     = ref<Metrics>({ cps:0, concurrency:0, asr:0, ccr:0, pdd:0, acd:0, failed:0 })
  const series      = ref({ cps:[] as number[], conc:[] as number[], asr:[] as number[], ccr:[] as number[], pdd:[] as number[], fail:[] as number[], mos:[] as number[] })
  const callStates  = ref<CallStates>({ invite:0, trying:0, ringing:0, ok:0, ack:0, bye:0, error:0 })
  const rtpMetrics  = ref<RtpMetrics>({ enabled:false, mos:0, packetLoss:0, jitter:0, packetsSent:0, packetsRecv:0, outOfOrder:0 })
  const respCodes   = ref<Record<string, number>>({ '100':0,'180':0,'200':0,'486':0,'404':0,'503':0,'408':0 })
  const logs        = ref<LogEntry[]>([])
  const flowTimes   = ref({ invite:'—', trying:'—', ringing:'—', ok:'—', ack:'—', bye:'—', done:'—' })
  const accountImportError = ref('')

  let simTimer:   ReturnType<typeof setInterval> | null = null
  let clockTimer: ReturnType<typeof setInterval> | null = null
  let tick = 0

  // ── Computed ──────────────────────────────────────────────────────────────────
  const mosRating = computed(() => {
    const m = rtpMetrics.value.mos
    if (m >= 4.0) return 'excellent'
    if (m >= 3.0) return 'good'
    if (m >= 2.5) return 'fair'
    return 'poor'
  })

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
      return `./sipress -s ${c.server} --mode caller --number ${c.caller.accessNumber} -c ${c.caller.concurrency} --cps ${c.caller.cps} --duration ${c.duration} --transport ${tr}${audio}`
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
    const domain = config.value.agent.defaultDomain
    const lines  = text.split('\n').map(l => l.trim()).filter(l => l && !l.startsWith('#'))
    const result: SipAccount[] = []

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]

      // Skip header row if it looks like a CSV header
      if (i === 0 && /^(extension|ext|username|user)/i.test(line)) continue

      if (line.includes(',')) {
        // CSV: extension,username,password[,domain]
        const p = line.split(',').map(s => s.trim())
        if (p.length < 3) { accountImportError.value = `Line ${i+1}: need extension,username,password`; continue }
        result.push({ id:uid(), extension:p[0], username:p[1], password:p[2], domain:p[3]||domain, status:'idle' })
      } else if (line.includes(':')) {
        // user:pass[:domain]
        const p = line.split(':')
        result.push({ id:uid(), extension:p[0].trim(), username:p[0].trim(), password:p[1]?.trim()??'', domain:p[2]?.trim()||domain, status:'idle' })
      } else {
        // space: ext user pass [domain]
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

  // ── Simulation ────────────────────────────────────────────────────────────────
  function _simTick() {
    const maxConc   = activeConcurrency.value
    const targetCps = activeCps.value

    const cps  = clamp(gauss(targetCps * 0.98, 0.7), 0, targetCps * 2)
    const conc = Math.round(clamp(gauss(maxConc * 0.97, 5), 0, maxConc * 1.2))
    const asr  = clamp(gauss(88, 3), 0, 100)
    const ccr  = clamp(gauss(82, 4), 0, 100)
    const pdd  = clamp(gauss(140, 28), 20, 800)
    const acd  = clamp(gauss(config.value.mode === 'caller' ? 30 : 180, 10), 5, 600)
    const fail = Math.random() < 0.06 ? ri(0, 3) : 0

    metrics.value = {
      cps: parseFloat(cps.toFixed(1)),
      concurrency: conc,
      asr: parseFloat(asr.toFixed(1)),
      ccr: parseFloat(ccr.toFixed(1)),
      pdd: parseFloat(pdd.toFixed(0)),
      acd: parseFloat(acd.toFixed(1)),
      failed: (metrics.value.failed ?? 0) + fail,
    }

    pushSeries('cps', cps); pushSeries('conc', conc)
    pushSeries('asr', asr); pushSeries('ccr', ccr)
    pushSeries('pdd', pdd); pushSeries('fail', fail)

    if (config.value.caller.enableAudio && config.value.caller.audioFile) {
      const mos = clamp(gauss(3.8, 0.3), 1, 5)
      rtpMetrics.value = {
        enabled: true,
        mos: parseFloat(mos.toFixed(2)),
        packetLoss: clamp(gauss(0.5, 0.4), 0, 20),
        jitter: clamp(gauss(18, 8), 0, 150),
        packetsSent: (rtpMetrics.value.packetsSent ?? 0) + Math.round(cps * 50),
        packetsRecv: (rtpMetrics.value.packetsRecv ?? 0) + Math.round(cps * 49),
        outOfOrder: rtpMetrics.value.outOfOrder + (Math.random() < 0.05 ? 1 : 0),
      }
      pushSeries('mos', mos)
    }

    callStates.value = {
      invite:Math.round(conc*0.07), trying:Math.round(conc*0.05),
      ringing:Math.round(conc*0.12), ok:Math.round(conc*0.63),
      ack:Math.round(conc*0.04), bye:Math.round(conc*0.06), error:Math.round(conc*0.03),
    }

    respCodes.value['100'] += Math.round(cps)
    respCodes.value['180'] += Math.round(cps * 0.88)
    respCodes.value['200'] += Math.round(cps * (asr / 100))
    if (Math.random() < 0.08) respCodes.value['486'] += ri(1,3)
    if (Math.random() < 0.03) respCodes.value['404']++
    if (Math.random() < 0.02) respCodes.value['503']++
    if (Math.random() < 0.01) respCodes.value['408']++

    if (config.value.mode === 'agent') {
      config.value.agent.accounts.forEach(acc => {
        if (acc.status === 'registering' && Math.random() < 0.3)
          acc.status = Math.random() < 0.95 ? 'registered' : 'failed'
      })
    }

    if (tick % 8 === 0) {
      flowTimes.value = {
        invite:'00:00', trying:`+${ri(2,15)}ms`, ringing:`+${ri(30,200)}ms`,
        ok:`+${ri(50,350)}ms`, ack:`+${ri(1,5)}ms`, bye:`+${ri(5,60)*1000}ms`, done:`+${ri(1,10)}ms`,
      }
    }

    const LOG_T: [LogEntry['level'], string][] = config.value.mode === 'caller'
      ? [
          ['ok',   `INVITE → ${config.value.caller.accessNumber}@${config.value.server}  Call-ID: ${randId()}`],
          ['ok',   `200 OK ← sip:${randIp()}  PDD: ${pdd.toFixed(0)}ms`],
          ['info', `180 Ringing ← PDD: ${pdd.toFixed(0)}ms`],
          ['ok',   `BYE →  duration: ${ri(5,60)}s`],
          ['warn', '486 Busy Here ← INVITE'],
          ['err',  '503 Service Unavailable ← INVITE'],
        ]
      : [
          ['ok',   `REGISTER ${config.value.agent.accounts[ri(0,Math.max(0,config.value.agent.accounts.length-1))]?.username??'agent'}@${config.value.agent.defaultDomain}`],
          ['ok',   '200 OK ← REGISTER  Expires: 3600'],
          ['info', `INVITE ← ${randIp()}  Call-ID: ${randId()}`],
          ['ok',   '200 OK → INVITE  agent answered'],
          ['warn', `REGISTER failed  401 Unauthorized  ext: ${1000+ri(0,20)}`],
        ]

    if (tick % 2 === 0) {
      const [lv, msg] = LOG_T[ri(0, LOG_T.length - 1)]
      addLog(lv, msg)
    }
    tick++
  }

  // ── Start / Stop ──────────────────────────────────────────────────────────────
  function startTest() {
    if (status.value === 'running') return
    status.value = 'running'; elapsedSec.value = 0
    metrics.value  = { cps:0, concurrency:0, asr:0, ccr:0, pdd:0, acd:0, failed:0 }
    series.value   = { cps:[], conc:[], asr:[], ccr:[], pdd:[], fail:[], mos:[] }
    rtpMetrics.value = { enabled:false, mos:0, packetLoss:0, jitter:0, packetsSent:0, packetsRecv:0, outOfOrder:0 }
    respCodes.value= { '100':0,'180':0,'200':0,'486':0,'404':0,'503':0,'408':0 }
    callStates.value = { invite:0, trying:0, ringing:0, ok:0, ack:0, bye:0, error:0 }
    tick = 0

    if (config.value.mode === 'agent') {
      config.value.agent.accounts.forEach(a => { a.status = 'registering' })
      addLog('info', `registering ${config.value.agent.accounts.length} SIP agents...`)
    }
    addLog('ok',   `test started → ${config.value.server}  [${config.value.mode} mode]`)
    addLog('info', `duration: ${config.value.duration}s`)

    simTimer   = setInterval(_simTick, 1000)
    clockTimer = setInterval(() => {
      elapsedSec.value++
      if (elapsedSec.value >= config.value.duration) stopTest()
    }, 1000)
  }

  function stopTest() {
    if (simTimer)   { clearInterval(simTimer);   simTimer   = null }
    if (clockTimer) { clearInterval(clockTimer); clockTimer = null }
    status.value = 'done'
    if (config.value.mode === 'agent')
      config.value.agent.accounts.forEach(a => { a.status = 'idle' })
    addLog('warn', `test stopped  elapsed: ${elapsedSec.value}s`)
  }

  // ── Export ────────────────────────────────────────────────────────────────────
  function exportJson() {
    const blob = new Blob([JSON.stringify({ mode:config.value.mode, target:config.value.server, elapsed:elapsedSec.value, config:config.value, metrics:metrics.value, respCodes:respCodes.value, series:series.value }, null, 2)], { type:'application/json' })
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = `sipress_${Date.now()}.json`; a.click()
    addLog('ok', 'report exported → sipress.json')
  }

  function exportCsv() {
    const rows = [['tick','cps','conc','asr','pdd','fail']]
    for (let i = 0; i < series.value.cps.length; i++)
      rows.push([String(i), series.value.cps[i]?.toFixed(2)?? '0', String(series.value.conc[i]??0), series.value.asr[i]?.toFixed(2)?? '0', series.value.pdd[i]?.toFixed(0)?? '0', String(series.value.fail[i]??0)])
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
    callStates, respCodes, logs, flowTimes,
    rtpMetrics, mosRating,
    accountImportError, registeredCount,
    activeConcurrency, activeCps, cliCommand, progressPct,
    addLog, clearLog,
    generateAccounts, removeAccount, clearAccounts, updateAccount,
    importAccountText, parseAccountText,
    startTest, stopTest,
    exportJson, exportCsv, exportAccountsCsv,
  }
})
