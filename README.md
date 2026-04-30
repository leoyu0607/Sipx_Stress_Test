# sipress — SIP 軟體交換機壓測工具

> 高效能、跨平台的 SIP UAC 壓測工具，支援真實 RTP 音訊傳送、聲音品質分析與 HTML 報告產出。

## 目錄

- [功能特色](#功能特色)
- [專案結構](#專案結構)
- [SIP 通話流程](#sip-通話流程)
- [RTP 音訊流程](#rtp-音訊流程)
- [關鍵指標](#關鍵指標)
- [快速開始](#快速開始)
- [CLI 參數完整說明](#cli-參數完整說明)
- [輸出格式](#輸出格式)
- [SIP Log](#sip-log)
- [Zig 靜態編譯](#zig-靜態編譯)
- [模組說明](#模組說明)

---

## 功能特色

| 功能 | 說明 |
|------|------|
| **SIP 信令** | 完整 INVITE / 100 / 180 / 200 / ACK / BYE / CANCEL 流程（RFC 3261） |
| **真實 RTP** | 每 20ms 傳送 G.711 PCMU 封包，支援 WAV 音檔循環播放 |
| **聲音品質分析** | MOS 估算（ITU-T E-Model G.107）、掉包率、Jitter（RFC 3550） |
| **即時 TUI** | ratatui 儀表板，顯示即時 ASR、CPS、並發數 |
| **HTML 報告** | 淺色主題，含環形指標圖、延遲分位數長條圖、RTP 品質區塊 |
| **SIP Log** | 每次壓測自動產生帶時間戳記的完整 SIP 訊息 log |
| **靜態編譯** | cargo-zigbuild 交叉編譯，無需 Docker，支援 Linux / Windows / macOS |
| **多輸出格式** | Table / JSON / CSV，可 pipe 給其他工具 |

---

## 專案結構

```
sipress/
├── Cargo.toml                    ← workspace
├── build.sh                      ← Zig 靜態交叉編譯腳本
├── .cargo/
│   └── config.toml               ← cargo-zigbuild 設定
│
├── core/                         ← 核心 library（無 UI）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── config.rs             ← 設定結構（Config / Transport）
│       ├── engine.rs             ← 壓測主引擎（並發通話控制）
│       ├── stats.rs              ← 指標收集（ASR/ACD/PDD/延遲/RTP）
│       ├── reporter.rs           ← 終端機輸出（Table / JSON / CSV）
│       ├── html_reporter.rs      ← HTML 報告產生器（淺色主題）
│       ├── sip_logger.rs         ← SIP 完整訊息 log 記錄器
│       ├── sip/
│       │   ├── mod.rs
│       │   ├── message.rs        ← SIP 訊息建構（INVITE/ACK/BYE/CANCEL）
│       │   ├── dialog.rs         ← SIP 對話狀態機
│       │   ├── parser.rs         ← SIP 回應解析（支援 header 縮寫）
│       │   └── transport.rs      ← UDP / TCP 傳輸層
│       └── rtp/
│           ├── mod.rs
│           ├── audio.rs          ← WAV 讀取 + G.711 μ-law 編碼
│           ├── packet.rs         ← RTP 封包建構與解析（RFC 3550）
│           ├── session.rs        ← Per-call RTP session（port 分配、收發）
│           └── stats.rs          ← Jitter / 掉包率 / MOS 計算
│
├── cli/                          ← TUI / CLI（Server 無頭環境）
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── args.rs               ← clap 參數定義
│       └── tui/
│           ├── mod.rs
│           └── dashboard.rs      ← ratatui 即時儀表板
│
└── gui/                          ← Tauri 桌面 / Android（開發中）
    ├── src-tauri/
    │   ├── Cargo.toml
    │   ├── main.rs
    │   └── commands.rs           ← Tauri bridge → core
    └── src/                      ← 前端 React/Vue
```

---

## SIP 通話流程

```
UAC (sipress)              UAS (軟交換機)
     │                          │
     │──── INVITE ─────────────▶│  從 --rtp-port 分配 RTP port，寫入 SDP
     │◀─── 100 Trying ──────────│
     │◀─── 180 Ringing ─────────│  PDD 計時結束
     │◀─── 200 OK ──────────────│  通話建立時間計時結束
     │──── ACK ────────────────▶│
     │                          │
     │═══ RTP G.711 音訊流 ════▶│  每 20ms 一個 160-byte PCMU frame
     │◀══ RTP G.711 音訊流 ═════│  接收端計算 Jitter / 掉包
     │                          │
     │──── BYE ────────────────▶│  通話持續時間計時結束，停止 RTP
     │◀─── 200 OK ──────────────│
```

---

## RTP 音訊流程

```
音檔（.wav / .raw）
       │
       ▼
   AudioSource          每 20ms 輸出一個 frame（160 bytes）
   ┌─────────────────────────────────────┐
   │  WAV PCM16 → 重採樣至 8kHz → μ-law │
   │  循環播放，通話結束自動停止          │
   └─────────────────────────────────────┘
       │
       ▼
   RtpPacket::encode()  組裝 RTP header（RFC 3550）
       │  PT=0(PCMU)  seq++  ts+=160  SSRC=random
       ▼
   UdpSocket::send()    傳送至對端 RTP port

接收端（同時執行）：
   UdpSocket::recv() → RtpPacket::decode()
       → RtpStats::on_recv()
           → Jitter（RFC 3550 §A.8）
           → 掉包率 = 1 − (recv / expected)
           → MOS（ITU-T E-Model G.107 簡化版）
```

### 支援音檔格式

| 格式 | 說明 |
|------|------|
| `.wav` | PCM 16-bit mono 或 stereo，8kHz / 16kHz（自動重採樣） |
| `.raw` / `.ul` / `.pcmu` | 原始 G.711 μ-law，8kHz mono |

---

## 關鍵指標

### SIP 指標

| 指標 | 說明 | 計算方式 |
|------|------|---------|
| **CPS** | Calls Per Second，每秒發起通話數 | `calls_initiated / duration` |
| **ASR** | Answer Seizure Ratio，接通率 | `calls_answered / calls_initiated × 100%` |
| **ACD** | Average Call Duration，平均通話時長 | HDR Histogram 均值（200 OK → BYE 200 OK） |
| **PDD** | Post Dial Delay，撥號後延遲 | INVITE 送出 → 收到 180 Ringing（ms） |
| **Setup Time** | 通話建立時間 | INVITE 送出 → 收到 200 OK（ms） |
| **CCR** | Call Completion Rate，通話完成率 | `calls_completed / calls_initiated × 100%` |
| **並發** | 同時維持中的通話數 | 瞬時計數 |

### RTP / 聲音品質指標

| 指標 | 說明 | 標準 |
|------|------|------|
| **MOS** | Mean Opinion Score，1.0 ~ 5.0 | ≥ 4.0 優、≥ 3.0 普通、< 2.5 差 |
| **掉包率** | Lost / Expected packets（%） | 電話品質建議 < 1%，可接受 < 3% |
| **Jitter** | 封包到達時間抖動（ms，RFC 3550 §A.8） | 建議 < 30ms |
| **亂序封包** | 序號倒退的封包數 | 網路品質指標 |

### MOS 估算公式（ITU-T E-Model 簡化）

```
Ie_eff = 0 + (95 − 0) × Ppl / (Ppl + 4.3)    ← 掉包影響（G.711 Bpl=4.3）
Id     = max(0, jitter_ms − 150) × 0.1         ← Jitter 影響（>150ms 才算）
R      = 93.2 − Ie_eff − Id                    ← R 值（0~100）
MOS    = 1 + 0.035R + R(R−60)(100−R) × 7×10⁻⁶ ← ITU-T G.107 §B.4
```

---

## 快速開始

### 安裝依賴

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 靜態編譯工具（可選）
cargo install cargo-zigbuild
pip install ziglang
```

### 編譯

```bash
# Debug（快速）
cargo build -p sipress

# Release
cargo build -p sipress --release
```

### 基本使用

```bash
# 最簡單：對 192.168.1.100:5060 發起 100 並發、10 CPS、持續 60 秒
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60

# 帶 TUI 即時儀表板
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 --tui

# 啟用 RTP 傳送（靜音）
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 --rtp

# 啟用 RTP + 播放音檔（模擬真實對話）
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 \
  --rtp --audio /path/to/sample.wav

# 自訂 RTP 起始 port
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 \
  --rtp --rtp-port 20000 --audio sample.wav

# 輸出 HTML 報告
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 \
  --rtp --audio sample.wav --html

# JSON 輸出（適合 CI/CD 整合）
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 --format json

# CSV 輸出（匯入試算表）
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 --format csv > result.csv
```

---

## CLI 參數完整說明

### SIP 連線

| 參數 | 簡短 | 預設 | 說明 |
|------|------|------|------|
| `--server` | `-s` | `127.0.0.1:5060` | 目標 SIP 伺服器（`ip:port`） |
| `--local` | — | 自動 | 本機綁定 IP |
| `--domain` | — | 自動（取本機 IP） | 本機 SIP domain（用於 From header） |

### 通話控制

| 參數 | 簡短 | 預設 | 說明 |
|------|------|------|------|
| `--concurrent` | `-c` | `100` | 最大並發通話數 |
| `--cps` | — | `10.0` | 每秒發起通話數（Calls Per Second） |
| `--duration` | `-d` | `60` | 壓測持續時間（秒） |
| `--call-duration` | — | `30` | 單通通話持續時間（秒，`0` = 不主動 BYE） |
| `--invite-timeout` | — | `8` | INVITE 逾時秒數（未收到回應視為失敗） |

### 號碼設定

| 參數 | 預設 | 說明 |
|------|------|------|
| `--from` | `1000` | 主叫號碼（SIP From header） |
| `--to-prefix` | `2` | 被叫號碼前綴 |
| `--to-range` | `9999` | 被叫尾數最大值（`0..=N` 隨機） |

> **範例**：`--to-prefix 86 --to-range 9999999` 會隨機撥打 `86xxxxxxx`

### RTP 音訊

| 參數 | 預設 | 說明 |
|------|------|------|
| `--rtp` | 關閉 | 啟用 RTP 音訊傳送與接收 |
| `--rtp-port` | `10000` | RTP 起始 port（偶數，每通 call +2） |
| `--audio` | 無（靜音） | 音檔路徑（`.wav` / `.raw` G.711 μ-law） |

### 輸出與記錄

| 參數 | 預設 | 說明 |
|------|------|------|
| `--tui` | 關閉 | 啟用 ratatui 即時儀表板 |
| `--format` | `table` | 終端機輸出格式：`table` / `json` / `csv` |
| `--html` | 關閉 | 產生 HTML 視覺化報告 |
| `--report-dir` | `reports/` | HTML 報告輸出目錄 |
| `--logs-dir` | `logs/` | SIP log 輸出目錄 |
| `--verbose` | 關閉 | 顯示詳細 debug log（含每秒進度） |

---

## 輸出格式

### Table（預設）

```
╔══════════════════════════════════════════════════╗
║              sipress 壓測報告                    ║
╠══════════════════════════════════════════════════╣
║  測試時長          60.0 s                        ║
╠══════════════════════════════════════════════════╣
║  發起通話           600                          ║
║  接通通話           522                          ║
║  完成通話           498                          ║
║  失敗通話            48                          ║
║  逾時通話            30                          ║
╠══════════════════════════════════════════════════╣
║  ASR              87.00 %                        ║
║  CCR              83.00 %                        ║
║  實際 CPS          9.87                          ║
║  ACD              28.40 s                        ║
╠══════════════════════════════════════════════════╣
║  PDD p50         142.0 ms                        ║
║  PDD p95         380.0 ms                        ║
║  ...                                             ║
╚══════════════════════════════════════════════════╝
```

### HTML 報告

壓測完成後自動產生於 `reports/YYYYMMDD_HHMMSS_report.html`，包含：

- **核心 KPI 卡片**：ASR、CCR、CPS、ACD、失敗合計
- **環形接通率圖**：ASR / CCR SVG 環形圖
- **通話分佈堆疊條**：完成 / 接通中 / 失敗 / 逾時比例
- **延遲分位數長條圖**：PDD 與 Setup Time（P50 / P95 / P99 / MAX）
- **RTP 聲音品質區塊**：MOS、掉包率、Jitter、封包統計
- **SIP 錯誤碼明細表**：4xx / 5xx / 6xx 各分類說明與建議

> 未啟用 `--rtp` 時，聲音品質區塊顯示「未啟用 RTP」提示。

---

## SIP Log

每次壓測自動在 `logs/`（可用 `--logs-dir` 調整）目錄建立一個 log 檔案。

### 檔名格式

```
logs/
└── YYYYMMDD_HHMMSS_agent.sip.log
```

| 欄位 | 說明 |
|------|------|
| `YYYYMMDD_HHMMSS` | 壓測開始的 UTC 時間（例如 `20240501_143022`） |
| `agent` | 壓測發起方（UAC）；預留 `user` 供被叫模擬擴充 |

### Log 格式範例

```
# sipress SIP Log
# 檔案建立時間：20240501_143022
# 角色：agent
# ----------------------------------------

── 14:30:22.001 >>> 192.168.1.100:5060 [→ SERVER] ──
INVITE sip:20001@192.168.1.100:5060 SIP/2.0
Via: SIP/2.0/UDP 192.168.1.10:58432;branch=z9hG4bK-3f8a...
From: <sip:1000@192.168.1.10>;tag=a1b2c3d4
To: <sip:20001@192.168.1.100:5060>
Call-ID: 8f3a2b1c@192.168.1.10
CSeq: 1 INVITE
...

── 14:30:22.143 <<< 192.168.1.100:5060 [← SERVER] ──
SIP/2.0 180 Ringing
...

── 14:30:22.210 <<< 192.168.1.100:5060 [← SERVER] ──
SIP/2.0 200 OK
...

── 14:30:52.211 >>> 192.168.1.100:5060 [→ SERVER] ──
BYE sip:20001@192.168.1.100:5060 SIP/2.0
...

# ════════════════════════════════════════
# 壓測結束摘要
# ════════════════════════════════════════
發起=600 接通=522 完成=498 失敗=48 逾時=30 ASR=87.0% 時長=60.3s
```

---

## Zig 靜態編譯

不需要 Docker 或系統 C 工具鏈，使用 Zig 作為跨平台 linker。

### 安裝工具

```bash
cargo install cargo-zigbuild
pip install ziglang          # 或從 ziglang.org 下載 zig binary
```

### 編譯

```bash
./build.sh linux-x86         # → dist/sipress-linux-x86_64   （musl 靜態）
./build.sh linux-arm64       # → dist/sipress-linux-arm64    （musl 靜態）
./build.sh windows           # → dist/sipress-windows-x86_64.exe
./build.sh macos-x86         # → dist/sipress-macos-x86_64
./build.sh macos-arm64       # → dist/sipress-macos-arm64
./build.sh all               # → 全平台（Linux x86 + ARM64 + Windows）
```

### 特性

- **musl 靜態連結**（Linux）：單一執行檔，無 glibc 依賴，可直接複製到任何 Linux 主機執行
- **零系統依賴**：不需要 openssl、libssl、libc++ 等系統函式庫
- **小體積**：release + strip 後約 3–5 MB

---

## 模組說明

### `core/src/config.rs`

`Config` struct 定義所有壓測參數，實作 `Default`（可只覆寫需要的欄位）與 `Serialize/Deserialize`（供 Tauri GUI 傳遞設定用）。

### `core/src/engine.rs`

壓測主引擎，基於 Tokio 非同步執行：

- **Task 1（接收）**：單一 UDP recv 迴圈，解析 SIP 回應，發送至內部 channel
- **Task 2（進度）**：每秒觸發 `on_progress` callback（TUI / GUI 更新用）
- **主控迴圈**：① 處理 SIP 事件 → ② 掃描逾時/BYE → ③ 依 CPS 發起新通話 → ④ 判斷結束
- RTP session 在收到 200 OK 後啟動，BYE 200 OK 後停止並收集統計

### `core/src/rtp/`

| 檔案 | 說明 |
|------|------|
| `audio.rs` | 載入 WAV / raw，G.711 μ-law 編解碼，輸出 20ms frame |
| `packet.rs` | RFC 3550 RTP 封包 encode / decode（V=2，不含擴充 header） |
| `session.rs` | 動態 port 分配（從 `rtp_base_port` 找可用偶數 port），spawn 傳送/接收 task |
| `stats.rs` | RFC 3550 §A.8 Jitter，E-Model MOS，掉包率、亂序、重複封包統計 |

### `core/src/sip/`

| 檔案 | 說明 |
|------|------|
| `message.rs` | 手刻 SIP 訊息格式（不依賴外部 crate），靜態編譯友好 |
| `dialog.rs` | 狀態機：`Calling → Trying → Ringing → Connected → Terminating → Completed` |
| `parser.rs` | 解析 status code、Call-ID、To/From tag、CSeq method，支援 header 縮寫 |
| `transport.rs` | `SharedUdpSocket`：綁定 + connect，供多個 task 共用 |

### `core/src/sip_logger.rs`

非同步安全的 SIP log 寫入器（`Mutex<File>`），每則訊息帶有方向箭頭（`>>>` / `<<<`）和 `HH:MM:SS.mmm` 時間戳記，壓測結束自動寫入摘要。

### `core/src/html_reporter.rs`

純 Rust 字串樣板產生完整 HTML（不依賴 Tera / Handlebars），內嵌 CSS 與 SVG，無 CDN 依賴，可離線檢視。

### `cli/src/tui/dashboard.rs`

ratatui + crossterm 儀表板：進度 gauge、即時指標 table，按 `q` 或 `Esc` 退出（壓測仍在背景執行，結束後仍輸出報告）。

### `gui/src-tauri/src/commands.rs`

四個 Tauri command：

| Command | 說明 |
|---------|------|
| `start_test(config)` | 啟動壓測（非同步，立即回傳） |
| `stop_test()` | 手動停止 |
| `get_snapshot()` | 取得即時 `StatsSnapshot`（前端每秒輪詢） |
| `get_report()` | 取得 `FinalReport`（壓測完成後） |
