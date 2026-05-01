# sipress — SIP 軟體交換機壓測工具

> 高效能、跨平台的 SIP UAC 壓測工具，提供視窗 GUI、終端機 TUI 與 CLI 三種操作介面，支援真實 RTP 音訊傳送、聲音品質分析與 HTML 報告產出。

[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri-2-blue)](https://tauri.app)
[![License: MIT](https://img.shields.io/badge/License-MIT-green)](LICENSE)

## 目錄

- [功能特色](#功能特色)
- [取得執行檔](#取得執行檔)
- [專案結構](#專案結構)
- [介面說明](#介面說明)
- [SIP 通話流程](#sip-通話流程)
- [RTP 音訊流程](#rtp-音訊流程)
- [關鍵指標](#關鍵指標)
- [CLI 快速開始](#cli-快速開始)
- [CLI 參數完整說明](#cli-參數完整說明)
- [輸出格式](#輸出格式)
- [SIP Log](#sip-log)
- [建置說明](#建置說明)
- [模組說明](#模組說明)

---

## 功能特色

| 功能 | 說明 |
|------|------|
| **視窗 GUI** | Tauri 桌面應用，無邊框深色主題，即時圖表與 SIP 日誌顯示 |
| **終端機 TUI** | ratatui 儀表板，顯示即時 ASR、CPS、並發數（`--tui`） |
| **SIP 信令** | 完整 INVITE / 100 / 180 / 200 / ACK / BYE / CANCEL 流程（RFC 3261） |
| **真實 RTP** | 每 20ms 傳送 G.711 PCMU 封包，支援 WAV 音檔循環播放 |
| **聲音品質分析** | MOS 估算（ITU-T E-Model G.107）、掉包率、Jitter（RFC 3550） |
| **總通數上限** | 民眾模式可設定 `--max-calls N`，達上限後自動停止（不依時長） |
| **即時計數** | 儀表板同步顯示成功通數、失敗通數、佇列通數、Error Rate |
| **HTML 報告** | 含環形指標圖、延遲分位數長條圖、RTP 品質區塊，可離線檢視 |
| **SIP Log** | 每次壓測自動產生帶時間戳記的完整 SIP 訊息 log |
| **靜態編譯** | cargo-zigbuild 交叉編譯，無需 Docker，支援 Linux / Windows / macOS |
| **多輸出格式** | Table / JSON / CSV，可 pipe 給其他工具 |

---

## 取得執行檔

### 直接下載（推薦）

前往 [Releases](../../releases) 下載對應平台的版本，所有檔案均在 `dist/` 目錄：

| 平台 | 安裝版 | 免安裝版 |
|------|--------|---------|
| Windows | `sipress-gui-windows-x86_64-installer.msi` | `sipress-gui-windows-x86_64-portable.exe` |
| Windows | `sipress-gui-windows-x86_64-setup.exe`（NSIS） | — |
| Linux | `sipress-gui-linux-x86_64-installer.deb` | `sipress-gui-linux-x86_64-portable.AppImage` |
| macOS ARM | `sipress-gui-macos-arm64-installer.dmg` | `sipress-gui-macos-arm64-portable` |

> **CLI 執行檔**（無 GUI）：
> - Windows：`sipress-windows-x86_64-native.exe`（MSVC）或 `sipress-windows-x86_64.exe`（GNU zigbuild）
> - Linux：`sipress-linux-x86_64`
> - macOS：`sipress-macos-arm64`

### 自行建置

請參閱 [建置說明](#建置說明)。

---

## 專案結構

```
sipress/
├── Cargo.toml                    ← workspace（core + cli + gui）
├── build.sh                      ← Linux/macOS 建置腳本
├── build.ps1                     ← Windows PowerShell 建置腳本
├── build.bat                     ← Windows 命令提示字元入口
│
├── core/                         ← 核心 library（無 UI，CLI 與 GUI 共用）
│   └── src/
│       ├── config.rs             ← 設定結構（Config / Transport）
│       ├── engine.rs             ← 壓測主引擎（並發通話控制）
│       ├── stats.rs              ← 指標收集（ASR/ACD/PDD/延遲/RTP）
│       ├── reporter.rs           ← 終端機輸出（Table / JSON / CSV）
│       ├── html_reporter.rs      ← HTML 報告產生器
│       ├── sip_logger.rs         ← SIP 完整訊息 log 記錄器
│       ├── sip/
│       │   ├── message.rs        ← SIP 訊息建構（INVITE/ACK/BYE/CANCEL）
│       │   ├── dialog.rs         ← SIP 對話狀態機
│       │   ├── parser.rs         ← SIP 回應解析
│       │   └── transport.rs      ← UDP / TCP 傳輸層
│       └── rtp/
│           ├── audio.rs          ← WAV 讀取 + G.711 μ-law 編碼
│           ├── packet.rs         ← RTP 封包建構與解析（RFC 3550）
│           ├── session.rs        ← Per-call RTP session（port 分配、收發）
│           └── stats.rs          ← Jitter / 掉包率 / MOS 計算
│
├── cli/                          ← 終端機 CLI + TUI 儀表板
│   └── src/
│       ├── main.rs
│       ├── args.rs               ← clap 參數定義
│       └── tui/dashboard.rs      ← ratatui 即時儀表板
│
└── gui/                          ← Tauri 桌面 GUI
    ├── src-tauri/
    │   ├── Cargo.toml
    │   ├── tauri.conf.json       ← 無邊框視窗、bundle 設定
    │   └── src/
    │       ├── lib.rs            ← Tauri 應用入口
    │       └── commands.rs       ← start_test / stop_test / get_snapshot / get_report
    └── src/                      ← Vue 3 + TypeScript 前端
        ├── components/           ← TitleBar / Sidebar / ChartPanel / LogPanel…
        └── stores/testStore.ts   ← 狀態管理 + Tauri invoke 橋接
```

---

## 介面說明

### GUI（視窗模式）

執行 `sipress-gui-*-portable.exe` 或安裝後開啟，可看到：

- **左側 Sidebar**：填寫伺服器位址、CPS、並發數、**總通數上限**、持續時間（**0 = 不限時間**）、音檔路徑
- **頂部 TitleBar**：顯示狀態、進度條，▶ Start / ■ Stop 按鈕
- **中間 MetricStrip**：即時顯示 CPS、CONCUR、**SUCCESS（成功通數）**、**FAILED（失敗通數）**、**QUEUED（佇列通數）**、ASR、**ERR%（Error Rate）**、PDD
- **圖表區**：折線圖（CPS / ASR / CCR / ERR 趨勢）
- **右側面板**：回應碼統計、RTP 品質（**MOS / 掉包率 / Jitter**）、SIP flow 時序
- **底部 LogPanel**：即時 SIP 事件日誌（color-coded）

> 詳細使用步驟請參閱 [howtouse.md](howtouse.md)

### TUI（終端機圖形介面）

```bash
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 --tui
```

按 `q` 或 `Esc` 退出（壓測仍在背景執行，結束後輸出報告）。

### CLI（純文字輸出）

```bash
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60
```

---

## SIP 通話流程

```
UAC (sipress)              UAS (軟交換機)
     │                          │
     │──── INVITE ─────────────▶│  本機 RTP port 寫入 SDP m= 行
     │◀─── 100 Trying ──────────│
     │◀─── 180 Ringing ─────────│  PDD 計時結束
     │◀─── 200 OK ──────────────│  通話建立，解析對端 RTP port
     │──── ACK ────────────────▶│
     │                          │
     │═══ RTP G.711 音訊流 ════▶│  每 20ms 一個 160-byte PCMU frame
     │◀══ RTP G.711 音訊流 ═════│  計算 Jitter / 掉包 / MOS
     │                          │
     │──── BYE ────────────────▶│  通話持續時間計時結束，停止 RTP
     │◀─── 200 OK ──────────────│
     │
     │  （INVITE 逾時未收到回應時）
     │──── CANCEL ─────────────▶│  RFC 3261 §9
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
   RtpPacket::encode()  PT=0(PCMU)  seq++  ts+=160  SSRC=random
       │
       ▼
   UdpSocket::send()    傳送至對端 RTP port（從 200 OK SDP 解析）

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
| **CPS** | Calls Per Second，每秒發起通話數 | 每秒快照差值 |
| **SUCCESS** | 目前成功接通通數 | `calls_answered`（累計） |
| **FAILED** | 目前失敗通數 | `calls_failed + calls_timeout`（累計） |
| **QUEUED** | 目前佇列中（進行中）通話數 | `calls_initiated - calls_completed - calls_failed - calls_timeout` |
| **ASR** | Answer Seizure Ratio，接通率 | `calls_answered / calls_initiated × 100%` |
| **CCR** | Call Completion Rate，完成率 | `calls_completed / calls_initiated × 100%` |
| **ERR%** | Error Rate，失敗率 | `(calls_failed + calls_timeout) / calls_initiated × 100%` |
| **ACD** | Average Call Duration，平均通話時長 | HDR Histogram 均值（200 OK → BYE 200 OK） |
| **PDD** | Post Dial Delay，撥號後延遲 | INVITE 送出 → 收到 180 Ringing（ms） |
| **Setup Time** | 通話建立時間 | INVITE 送出 → 收到 200 OK（ms） |
| **CCR** | Call Completion Rate，通話完成率 | `calls_completed / calls_initiated × 100%` |

### RTP / 聲音品質指標

| 指標 | 說明 | 標準 |
|------|------|------|
| **MOS** | Mean Opinion Score，1.0 ~ 5.0 | ≥ 4.0 優、≥ 3.0 普通、< 2.5 差 |
| **掉包率** | Lost / Expected packets（%） | 建議 < 1%，可接受 < 3% |
| **Jitter** | 封包到達時間抖動（ms，RFC 3550 §A.8） | 建議 < 30ms |

### MOS 估算公式（ITU-T E-Model 簡化）

```
Ie_eff = 0 + (95 − 0) × Ppl / (Ppl + 4.3)    ← 掉包影響（G.711 Bpl=4.3）
Id     = max(0, jitter_ms − 150) × 0.1         ← Jitter 影響（>150ms 才算）
R      = 93.2 − Ie_eff − Id
MOS    = 1 + 0.035R + R(R−60)(100−R) × 7×10⁻⁶  ← ITU-T G.107 §B.4
```

---

## CLI 快速開始

```bash
# 最簡單：對 192.168.1.100:5060 發起 100 並發、10 CPS、持續 60 秒
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60

# 帶 TUI 即時儀表板
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 --tui

# 啟用 RTP + 播放音檔
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 \
  --rtp --audio /path/to/sample.wav

# 輸出 HTML 報告
./sipress -s 192.168.1.100:5060 -c 100 --cps 10 -d 60 --html

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
| `--domain` | — | 自動 | 本機 SIP domain（用於 From header） |

### 通話控制

| 參數 | 簡短 | 預設 | 說明 |
|------|------|------|------|
| `--concurrent` | `-c` | `100` | 最大並發通話數 |
| `--cps` | — | `10.0` | 每秒發起通話數 |
| `--duration` | `-d` | `60` | 壓測持續時間（秒）；**設為 `0` = 不限時間**，需搭配 `--max-calls` 或手動停止 |
| `--max-calls` | — | 不限 | 總通話上限，達到且所有進行中通話結束後自動停止 |
| `--call-duration` | — | `30` | 單通通話持續時間（秒，`0` = 不主動 BYE） |
| `--invite-timeout` | — | `8` | INVITE 逾時秒數 |

### 號碼設定

| 參數 | 預設 | 說明 |
|------|------|------|
| `--from` | `1000` | 主叫號碼 |
| `--to-prefix` | `2` | 被叫號碼前綴 |
| `--to-range` | `9999` | 被叫尾數最大值（隨機） |

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
| `--format` | `table` | 輸出格式：`table` / `json` / `csv` |
| `--html` | 關閉 | 產生 HTML 視覺化報告 |
| `--report-dir` | `reports/` | HTML 報告輸出目錄 |
| `--logs-dir` | `logs/` | SIP log 輸出目錄 |
| `--verbose` | 關閉 | 顯示詳細 debug log |

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
║  接通通話           522    ASR    87.00 %         ║
║  完成通話           498    CCR    83.00 %         ║
║  失敗通話            48    CPS     9.87           ║
║  逾時通話            30    ACD    28.40 s         ║
╠══════════════════════════════════════════════════╣
║  PDD p50  142ms   p95  380ms   p99  510ms        ║
╚══════════════════════════════════════════════════╝
```

### HTML 報告

壓測完成後產生於 `reports/YYYYMMDD_HHMMSS_report.html`，包含：

- 核心 KPI 卡片（ASR、CCR、CPS、ACD）
- 環形接通率 SVG 圖
- 延遲分位數長條圖（PDD P50/P95/P99/MAX）
- RTP 聲音品質區塊（MOS、掉包率、Jitter）
- SIP 錯誤碼明細表（4xx / 5xx / 6xx）

---

## SIP Log

每次壓測自動在 `logs/` 目錄建立 `YYYYMMDD_HHMMSS_agent.sip.log`，記錄完整 SIP 訊息（帶方向箭頭與毫秒時間戳記），壓測結束自動寫入統計摘要。

---

## 建置說明

### 前置需求

| 工具 | 用途 | 安裝 |
|------|------|------|
| Rust + Cargo | 編譯核心與 CLI | `rustup.rs` |
| cargo-zigbuild | 跨平台靜態編譯 | `cargo install cargo-zigbuild` |
| zig / ziglang | cargo-zigbuild linker | `pip install ziglang` |
| Node.js + npm | 建置 Tauri GUI 前端 | `nodejs.org` |

### 建置腳本（一鍵建置）

**Windows：**
```bat
build.bat gui              ← 只建 GUI
build.bat windows-native   ← 只建 CLI
build.bat all              ← 全部（CLI 所有平台 + GUI）
```

**Linux / macOS：**
```bash
bash build.sh gui          # 只建 GUI
bash build.sh linux-x86    # 只建 Linux CLI
bash build.sh all          # 全部
```

### 所有輸出都在 `dist/`

> 不同平台執行 `all` 產生的檔案不同：GUI bundle 只會在當前平台建置，CLI cross-compile 產出固定。

```
dist/
│
│  ── GUI（建置平台決定） ──
├── sipress-gui-windows-x86_64-installer.msi    ← GUI Windows 安裝版（Windows 建置）
├── sipress-gui-windows-x86_64-setup.exe        ← GUI Windows 安裝版 NSIS（Windows 建置）
├── sipress-gui-windows-x86_64-portable.exe     ← GUI Windows 免安裝版（Windows 建置）
├── sipress-gui-linux-x86_64-installer.deb      ← GUI Linux 安裝版（Linux 建置）
├── sipress-gui-linux-x86_64-portable.AppImage  ← GUI Linux 免安裝版（Linux 建置）
├── sipress-gui-macos-arm64-installer.dmg       ← GUI macOS 安裝版（macOS 建置）
├── sipress-gui-macos-arm64-portable            ← GUI macOS 免安裝版（macOS 建置）
│
│  ── CLI（zigbuild 交叉編譯，任何平台皆可產生） ──
├── sipress-linux-x86_64                        ← CLI Linux x86_64 靜態二進位
├── sipress-linux-arm64                         ← CLI Linux ARM64 靜態二進位
├── sipress-windows-x86_64.exe                  ← CLI Windows x86_64（GNU zigbuild）
│
│  ── CLI（native，建置平台決定） ──
├── sipress-windows-x86_64-native.exe           ← CLI Windows MSVC（Windows 建置）
├── sipress-macos-arm64                         ← CLI macOS ARM64（macOS 建置）
└── sipress-linux-x86_64                        ← CLI Linux x86_64（Linux 建置，與 zigbuild 同名）
```

### 僅建置 CLI

```bash
# Debug
cargo build -p sipress

# Release
cargo build -p sipress --release
```

### 僅建置 GUI（開發模式）

```bash
cd gui
npm install
npm run tauri dev   # 開啟視窗，hot-reload 前端
```

---

## 模組說明

### `core/src/engine.rs`

壓測主引擎（Tokio 非同步）：
- **接收 task**：單一 UDP recv 迴圈，解析回應發至 channel
- **進度 task**：每秒觸發 `on_progress` callback（TUI / GUI 更新）
- **主控迴圈**：處理 SIP 事件 → 掃描逾時 → 依 CPS 發起新通話 → 判斷結束
- RTP session 在收到 200 OK 後啟動，BYE 後停止並收集統計

### `core/src/sip/message.rs`

手刻 SIP 訊息格式（不依賴外部 crate）：
- `SipMessage::invite()` — 含 SDP，`m=audio PORT RTP/AVP 0` 使用預分配的本機 port
- `SipMessage::cancel()` — RFC 3261 §9 CANCEL
- `SipResponse::sdp_rtp_port()` — 從 200 OK body 解析對端 RTP port

### `core/src/rtp/session.rs`

動態 port 分配（從 `rtp_base_port` 掃描 4000 個偶數 port），**在送出 INVITE 之前**就分配好本機 port 並寫入 SDP，收到 200 OK 後才真正啟動 RTP。

### `gui/src-tauri/src/commands.rs`

四個 Tauri command，前端透過 `invoke()` 呼叫：

| Command | 說明 |
|---------|------|
| `start_test(config)` | 啟動壓測（背景非同步，立即回傳） |
| `stop_test()` | 手動停止 |
| `get_snapshot()` | 取得即時 `StatsSnapshot`（前端每秒輪詢） |
| `get_report()` | 取得最終 `FinalReport`（壓測完成後） |
