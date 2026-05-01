# sipress 程式碼完整講解

本文件適合想深入理解 sipress 整體架構、各模組邏輯與關鍵注意事項的開發者。讀者需具備基本 Rust 知識。

## 目錄

1. [整體架構](#1-整體架構)
2. [Workspace 結構與編譯單元](#2-workspace-結構與編譯單元)
3. [core — 設定層 config.rs](#3-core--設定層-configrs)
4. [core — 指標層 stats.rs](#4-core--指標層-statsrs)
5. [core — SIP 訊息 sip/message.rs](#5-core--sip-訊息-sipmessagers)
6. [core — SIP 對話狀態機 sip/dialog.rs](#6-core--sip-對話狀態機-sipdialogrs)
7. [core — SIP 解析 sip/parser.rs & transport.rs](#7-core--sip-解析-sipparserrs--transportrs)
8. [core — 壓測主引擎 engine.rs](#8-core--壓測主引擎-enginers)
9. [core — RTP 音訊 rtp/audio.rs](#9-core--rtp-音訊-rtpaudiors)
10. [core — RTP 封包 rtp/packet.rs](#10-core--rtp-封包-rtppacketrs)
11. [core — RTP Session rtp/session.rs](#11-core--rtp-session-rtpsessionrs)
12. [core — RTP 品質統計 rtp/stats.rs](#12-core--rtp-品質統計-rtpstatsrs)
13. [core — 報告輸出 reporter.rs & html_reporter.rs](#13-core--報告輸出-reporterrs--html_reporterrs)
14. [core — SIP 日誌 sip_logger.rs](#14-core--sip-日誌-sip_loggerrs)
15. [cli — 命令列介面](#15-cli--命令列介面)
16. [gui — Tauri 後端 commands.rs & lib.rs](#16-gui--tauri-後端-commandsrs--librs)
17. [gui — 前端狀態管理 testStore.ts](#17-gui--前端狀態管理-teststorets)
18. [gui — Vue 元件樹](#18-gui--vue-元件樹)
19. [建置系統 build.ps1 / build.sh](#19-建置系統-buildps1--buildsh)
20. [關鍵設計決策與注意事項](#20-關鍵設計決策與注意事項)

---

## 1. 整體架構

```
使用者
  │
  ├─ GUI (Tauri) ──────────────── Rust process
  │    ├── Vue 3 + TypeScript       │
  │    │   (WebView)                │
  │    │   invoke('start_test')     │
  │    └──────────────────────────▶│ commands.rs
  │                                 │   Engine::run()
  ├─ CLI / TUI ─────────────────▶──┤
  │    args.rs + clap               │ core/engine.rs
  │                                 │   ├── UDP socket (SIP)
  └─ (未來：API / CI mode)          │   ├── UDP socket (RTP/per-call)
                                    │   └── StatsSnapshot → callback
```

sipress 遵循「一個 core library，多個 entry point」的設計：
- **`sipress-core`**：純 library crate，包含所有業務邏輯（SIP、RTP、統計、報告）
- **`sipress`（cli）**：二進位 crate，`main.rs` 解析 CLI 參數後建構 `Config` 呼叫 core
- **`sipress-gui`（gui）**：Tauri 桌面應用，`commands.rs` 作為 frontend/backend 橋接

這樣的分層確保 core 可被單獨測試，且 CLI 與 GUI 共用完全相同的引擎邏輯。

---

## 2. Workspace 結構與編譯單元

```toml
# Cargo.toml（workspace root）
[workspace]
members = ["core", "cli", "gui/src-tauri"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
```

**重點**：
- `resolver = "2"` 是 Rust 2021 edition 的要求，解決 feature 衝突問題
- workspace 共享 `tokio` 版本，避免多版本共存的連結問題
- Tauri 編譯的 `target/` 在 workspace root，不在 `gui/src-tauri/target/`
- 因此 build script 應從 `target/release/` 複製產出，而非 `gui/src-tauri/target/release/`

---

## 3. core — 設定層 config.rs

`Config` 是引擎的輸入資料結構，`Serialize/Deserialize` 讓它可以：
1. 從 CLI (`clap`) 組裝後直接傳入
2. 從 GUI 前端 JSON（透過 Tauri `invoke`）反序列化傳入

```rust
pub struct Config {
    pub server_addr:          String,       // "ip:port"
    pub cps:                  f64,          // calls per second
    pub max_concurrent_calls: usize,
    pub duration_secs:        u64,          // 0 = unlimited
    pub call_duration_secs:   u64,          // 0 = no auto-BYE
    pub invite_timeout_secs:  u64,          // 8s default
    pub max_total_calls:      Option<u64>,  // None = unlimited
    pub enable_rtp:           bool,
    pub audio_file:           Option<PathBuf>,
    // ...
}
```

**注意事項**：
- `duration_secs = 0` 表示「不限時間」；引擎內部以 `u64::MAX / 2` 作為遠期 deadline
- `max_total_calls = None` 表示不限通數，搭配 `duration_secs` 決定結束時機
- 兩者皆為「不限」時，測試必須手動停止（GUI: Stop 按鈕；CLI: Ctrl-C）
- `call_duration_secs = 0` 表示不主動送 BYE，通話由伺服器端掛斷

---

## 4. core — 指標層 stats.rs

### LiveStats（無鎖計數）

```rust
pub struct LiveStats {
    pub calls_initiated: AtomicU64,   // 已送出 INVITE 數
    pub calls_answered:  AtomicU64,   // 已收到 200 OK for INVITE 數
    pub calls_completed: AtomicU64,   // 已收到 200 OK for BYE 數（正常結束）
    pub calls_failed:    AtomicU64,   // 已收到 4xx/5xx/6xx 數
    pub calls_timeout:   AtomicU64,   // 已逾時（INVITE 無回應）數
    pub calls_active:    AtomicI64,   // 目前進行中通話數（有借有還）
}
```

`calls_active` 是 `AtomicI64`（有號整數）而非 `AtomicU64`，因為理論上在某些 race condition 下可能瞬間為負，用有號整數避免 underflow panic；取快照時用 `.max(0) as u64`。

**生命週期計數**：
- `on_invite()` → `calls_initiated++`, `calls_active++`
- `on_answered()` → `calls_answered++`（不動 active，通話仍在進行）
- `on_completed()` → `calls_completed++`, `calls_active--`
- `on_failed()` → `calls_failed++`, `calls_active--`
- `on_timeout()` → `calls_timeout++`, `calls_active--`

所有計數均使用 `Ordering::Relaxed`，因為只需最終一致性，不需要跨執行緒的 happens-before 保證。

### StatsSnapshot（可序列化快照）

```rust
pub struct StatsSnapshot {
    pub calls_concurrent: u64,   // = calls_active.max(0)
    pub asr:              f64,   // calls_answered / calls_initiated × 100
    pub error_rate:       f64,   // (calls_failed + calls_timeout) / calls_initiated × 100
}
```

`QUEUED`（佇列中）在前端計算：`calls_initiated - calls_answered - calls_failed - calls_timeout`，等價於「已發出但未接通也未失敗的 INVITE 數量」，近似於 `calls_active`。

### DetailedStats（直方圖，需 Mutex）

使用 `hdrhistogram::Histogram<u64>` 儲存 PDD / 通話建立時間 / 通話持續時間的分佈。

HDR Histogram 的特性：
- `new_with_bounds(1, 60_000_000, 3)` — 最小 1µs、最大 60s（60M µs）、3 位有效數字
- 精確記錄延遲百分位（P50/P95/P99/MAX）而不失精度
- 因 Histogram 本身不是 Sync，需包在 `Mutex` 中

**重要注意事項**：`std::sync::Mutex` guard 不能跨 `.await` 存活（`MutexGuard<T>` 不實作 `Send`）。engine.rs 中取得 histogram 數據的程式碼必須在所有 `.await` 完成後才取 lock：

```rust
// ✅ 正確：先完成所有 async 操作，再取 std::sync::MutexGuard
let rtp_agg = ... .await;  // 先做完所有 async
let pdd_p50 = {
    let pdd_h = detail.pdd_hist.lock().unwrap();  // 再取 Mutex
    us_to_ms(&pdd_h, 0.50)
};

// ❌ 錯誤：不能在 lock 之後有 .await
let pdd_h = detail.pdd_hist.lock().unwrap();
do_something().await;  // compile error: MutexGuard is not Send
```

---

## 5. core — SIP 訊息 sip/message.rs

sipress 不依賴任何 SIP 框架，手工構建 RFC 3261 格式字串。

### INVITE 構建

```
INVITE sip:{to}@{server} SIP/2.0
Via: SIP/2.0/UDP {local};branch={branch};rport
Max-Forwards: 70
From: <sip:{from}@{domain}>;tag={from_tag}
To: <sip:{to}@{server}>
Call-ID: {call_id}
CSeq: 1 INVITE
Contact: <sip:{from}@{local};transport=udp>
Content-Type: application/sdp
Content-Length: {len}
User-Agent: sipress/0.1
Allow: INVITE,ACK,BYE,CANCEL,OPTIONS

v=0
o=sipress 1000 1000 IN IP4 {local_ip}
s=sipress
c=IN IP4 {local_ip}
t=0 0
m=audio {rtp_port} RTP/AVP 0 8
a=rtpmap:0 PCMU/8000
a=rtpmap:8 PCMA/8000
a=sendrecv
```

**SDP 中的 RTP port**：
- 若啟用 RTP：填入預先分配好的本機偶數 port（例如 `10000`）
- 若未啟用 RTP：填入 `9`（SDP RFC 規定 port=9 表示媒體流被停用）

### 唯一識別碼生成

```rust
pub fn new_branch() -> String {
    format!("z9hG4bK-{}", Uuid::new_v4().simple())
}
```

RFC 3261 要求 branch 以 `z9hG4bK` 開頭（magic cookie），用以區別 RFC 2543 的舊格式。

```rust
pub fn new_call_id(domain: &str) -> String {
    format!("{}@{}", Uuid::new_v4().simple(), domain)
}
```

Call-ID 格式：`<uuid>@<local_domain>`，保證全局唯一。

### ACK 注意事項

ACK 的 CSeq 序號必須與對應的 INVITE **相同**（不是 +1）。這是 RFC 3261 §17.1.1.3 的規定，sipress 使用 `dialog.cseq`（初始值為 1）送 ACK，BYE 時才 `cseq + 1`。

---

## 6. core — SIP 對話狀態機 sip/dialog.rs

每一通通話對應一個 `Dialog` 實例，儲存在引擎的 `HashMap<String, Dialog>`（鍵為 Call-ID）。

### 狀態轉移圖

```
Calling ──(100)──▶ Trying
Calling/Trying ──(180)──▶ Ringing        [記錄 ringing_at，計算 PDD]
Calling/Trying/Ringing ──(200 INVITE)──▶ Connected  [記錄 answered_at，觸發 ACK + RTP]
Connected ──(通話時間到/BYE sent)──▶ Terminating
Terminating ──(200 BYE)──▶ Completed     [記錄 ended_at，計算 ACD]

Calling/Trying/Ringing ──(invite_timeout)──▶ TimedOut
Calling/Trying/Ringing/Connected ──(4xx/5xx/6xx)──▶ Failed(code)
```

### 計時點

| 指標 | 起始 | 結束 |
|------|------|------|
| PDD | `invite_sent_at` | `ringing_at` |
| Setup Time | `invite_sent_at` | `answered_at` |
| Call Duration（ACD）| `answered_at` | `ended_at`（BYE 200 OK） |

### Dialog 清理

引擎主迴圈每輪掃描後，`retain` 只保留「活躍」狀態：

```rust
dialogs.retain(|_, d| matches!(
    d.state,
    DialogState::Calling | DialogState::Trying |
    DialogState::Ringing | DialogState::Connected |
    DialogState::Terminating
));
```

`Completed`、`Failed`、`TimedOut` 的 dialog 在統計記錄後即被清除，避免 HashMap 無限增長。

---

## 7. core — SIP 解析 sip/parser.rs & transport.rs

### SIP 回應解析（message.rs 中的 SipResponse）

sipress 只需要解析回應（UAC 模式，不處理請求）：

```rust
pub fn status_code(raw: &str) -> Option<u16> {
    // 取第一行，格式：SIP/2.0 200 OK
    let line = raw.lines().next()?;
    line.splitn(3, ' ').nth(1)?.parse().ok()
}
```

**重要**：解析函數全部是 `Option<T>` 回傳，不 panic。原始 UDP 封包可能損毀或截斷，必須容錯。

### CSeq Method 解析

```rust
pub fn cseq_method(raw: &str) -> Option<String> {
    // CSeq: 1 INVITE  → 取第二個 token "INVITE"
    for line in raw.lines() {
        if line.to_lowercase().starts_with("cseq:") {
            return line[5..].trim().split_whitespace().nth(1)
                .map(|s| s.to_uppercase());
        }
    }
    None
}
```

為什麼需要 CSeq Method？因為 200 OK 可能是對 INVITE 或 BYE 的回應，兩者處理邏輯完全不同。

### SDP RTP Port 解析

```rust
pub fn sdp_rtp_port(raw: &str) -> Option<u16> {
    // 找到 SIP/SDP 分界（空行 \r\n\r\n）
    let body_start = raw.find("\r\n\r\n").map(|i| i + 4)
        .or_else(|| raw.find("\n\n").map(|i| i + 2))?;
    // m=audio 16384 RTP/AVP 0  → 取第二個 token "16384"
    for line in raw[body_start..].lines() {
        if line.starts_with("m=") {
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if let Ok(port) = parts.get(1)?.parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}
```

### transport.rs — SharedUdpSocket

所有 SIP 訊息（INVITE、ACK、BYE、CANCEL）共用同一個 UDP socket，以 `Arc` 共享給各個 tokio task：

```rust
pub struct SharedUdpSocket {
    pub socket:     UdpSocket,
    pub server:     SocketAddr,
    pub local_addr: String,  // "ip:port"（填入 SIP Via header）
}
```

`UdpSocket::connect()` 設定預設的對端地址，後續 `send()` 不需再指定目標。

---

## 8. core — 壓測主引擎 engine.rs

引擎是 sipress 最複雜的部分，採用 **Tokio 單執行緒事件迴圈**模型。

### 並發架構

```
tokio::spawn ─── Task 1：接收 UDP 封包
                    │
                    │  mpsc::unbounded_channel
                    ▼
tokio::spawn ─── Task 2：進度回報（每秒）
                    │
                    ▼
主控迴圈（非同步，但不 spawn 新 task）
  ①  drain ev_rx（處理 SIP 事件）
  ②  掃描逾時 & 送 BYE
  ③  發起新通話（INVITE）
  ④  判斷是否結束
  ⑤  sleep 500µs
```

**為什麼不用多執行緒？**
- SIP 狀態管理需要頻繁修改 `HashMap<String, Dialog>`
- 用 `tokio::Mutex` 而非 `std::sync::Mutex`，在 `.await` 期間可以釋放 lock
- 單一主控迴圈避免了 race condition

### Task 1：接收迴圈

```rust
tokio::spawn(async move {
    let mut buf = vec![0u8; 65536];
    loop {
        let n = udp_recv.socket.recv(&mut buf).await?;
        let raw = String::from_utf8_lossy(&buf[..n]).into_owned();
        // 解析 → 送 SipEvent 到 channel
        let _ = ev_tx2.send(SipEvent::Response { call_id, code, ... });
    }
});
```

接收 task 只做最少的工作（解析狀態碼、Call-ID），不修改任何共享狀態，所有狀態更新都透過 channel 傳回主控迴圈。

### Task 2：進度回報

```rust
let unlimited = cfg.duration_secs == 0;
tokio::spawn(async move {
    let mut interval = time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let elapsed = start.elapsed().as_secs_f64();
        let progress = if unlimited { 0.0 } else { (elapsed / duration).min(1.0) };
        cb(live.snapshot(), progress);
        if !unlimited && elapsed >= duration { break; }
    }
});
```

當 `duration_secs = 0` 時：
- `progress` 固定回報 `0.0`（前端顯示 0% 進度）
- 迴圈不自動 break，等主控迴圈結束後 task 被 drop

### CPS 節流機制

```rust
let cps_interval = Duration::from_secs_f64(1.0 / cfg.cps);
let mut next_call = Instant::now();

// 主控迴圈內：
if !total_limit_reached && now >= next_call && now < deadline {
    if concurrent < cfg.max_concurrent_calls {
        // 送 INVITE
        next_call = now + cps_interval;
    }
}
```

這是一個簡單的令牌桶機制：每隔 `1/CPS` 秒才能發起一通新通話。若並發上限已滿則跳過這個時槽，但 `next_call` 不推進（下一輪立即再試）。

**注意**：若 CPS = 10 且並發滿了，新通話會等到有空間才送出，實際 CPS 可能低於設定值。這是正確行為。

### 結束條件邏輯

```rust
let unlimited_time = cfg.duration_secs == 0;
let deadline = if unlimited_time {
    start + Duration::from_secs(u64::MAX / 2)  // 遠期（約 2900 億年）
} else {
    start + cfg.duration()
};

// 每輪主控迴圈結束前：
let total_limit_reached = cfg.max_total_calls
    .map_or(false, |max| live.calls_initiated.load(...) >= max);

let all_done = total_limit_reached
    && live.calls_active.load(...) <= 0;

if now >= deadline || all_done {
    time::sleep(Duration::from_secs(2)).await;  // 等待最後幾個通話結束
    break;
}
```

| 結束條件 | 說明 |
|---------|------|
| `now >= deadline` | 時間到（duration > 0） |
| `total_limit_reached && calls_active <= 0` | 達通數上限且沒有進行中的通話 |
| stop channel 收到訊號（GUI Stop）| `tokio::select!` 中斷 engine |

### 主迴圈睡眠時間

```rust
time::sleep(Duration::from_micros(500)).await;
```

500µs 的睡眠足以支援最高 2000 CPS（1/2000s = 500µs）。若需要更高 CPS，應縮短睡眠時間或改為 busy-wait。

---

## 9. core — RTP 音訊 rtp/audio.rs

`AudioSource` 負責從音檔讀取並輸出 G.711 µ-law frames：

```
WAV (PCM16) ──▶ 重採樣至 8kHz ──▶ µ-law 編碼 ──▶ 160-byte frames（20ms @ 8kHz）
RAW / PCM µ-law ──▶ 直接使用
```

**WAV 解析**：使用 `hound` crate 讀取 PCM16。若取樣率不是 8kHz，進行線性重採樣。

**循環播放**：`next_frame()` 到達結尾後回到頭部，確保長時間測試持續有音訊輸出。

**靜音模式**：`AudioSource::silence()` 回傳全零的 frames，仍然送出 RTP 封包（正確的 silence suppression 模擬）。

---

## 10. core — RTP 封包 rtp/packet.rs

### RTP 標頭格式（RFC 3550）

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|V=2|P|X|  CC   |M|     PT      |       Sequence Number         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                           Timestamp                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                             SSRC                              |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                             Payload                           |
```

sipress 使用：
- `V=2`（版本 2）
- `P=0, X=0, CC=0`（無 padding、無擴展、無 CSRC）
- `M=0`（非語音開始）
- `PT=0`（PCMU, G.711 µ-law）
- `seq`：每封包 +1，初始值隨機（防止預測）
- `ts`：每封包 +160（20ms × 8kHz），初始值隨機
- `SSRC`：每個 RTP session 隨機產生

---

## 11. core — RTP Session rtp/session.rs

### Port 預分配機制

這是 sipress 中最重要的設計之一：**RTP port 在 INVITE 送出前就必須分配完成**，才能寫入 SDP。

```rust
// engine.rs 中，送 INVITE 前：
let rtp_port = if cfg.enable_rtp {
    RtpSession::allocate_port(&port_counter, &local_ip).await?
} else {
    0  // 未啟用 RTP，SDP 填 port=9（停用）
};

let invite = SipMessage::invite(..., rtp_port);
// SDP 中：m=audio {rtp_port} RTP/AVP 0 8
```

`allocate_port()` 掃描從 `rtp_base_port` 開始的偶數 port，嘗試 `bind()` 確認可用：

```rust
for offset in (0u16..4000).step_by(2) {
    let port = start.wrapping_add(offset);
    let addr = format!("{}:{}", local_ip, port);
    if UdpSocket::bind(&addr).await.is_ok() {
        *guard = port.wrapping_add(2);
        return Ok(port);
    }
}
```

**注意**：`bind()` 成功後立即釋放 socket，稍後在 `RtpSession::start()` 中再次綁定。這中間有極短的 race window，但在壓測場景下實際問題極少（系統不太可能在微秒內把剛釋放的 port 分配給別人）。

### 兩個並發 Task

`RtpSession::start()` spawn 兩個獨立 task：

**Task A（傳送）**：每 20ms tick 一次，讀取音訊 frame，構建 RTP 封包，送出 UDP：

```rust
let mut interval = time::interval(Duration::from_millis(20));
loop {
    interval.tick().await;
    if stop.load(Ordering::Relaxed) { break; }
    // ...送封包
    seq = seq.wrapping_add(1);
    ts  = ts.wrapping_add(160);
}
```

**Task B（接收）**：非阻塞接收，有 50ms timeout，定期檢查 stop_flag：

```rust
match time::timeout(Duration::from_millis(50), socket.recv(&mut buf)).await {
    Ok(Ok(n)) => { /* 計算 jitter/loss */ }
    Err(_) => {} // timeout，繼續迴圈以便檢查 stop_flag
}
```

### 停止機制

```rust
pub fn stop(&self) -> RtpStatsSnapshot {
    self.stop_flag.store(true, Ordering::Relaxed);
    self.stats.snapshot()
}
```

`stop()` 是同步函數（不 await），設置 flag 後立即回傳快照。兩個 task 會在下次迴圈迭代時檢測到 flag 並退出。

---

## 12. core — RTP 品質統計 rtp/stats.rs

### Jitter 計算（RFC 3550 §A.8）

RFC 3550 定義的 jitter 是**相鄰封包到達間隔的差異的指數加權移動平均**：

```
D(i,j) = |(Rj - Ri) - (Sj - Si)|    （均以 8kHz clock 計）

J(i) = J(i-1) + ( |D| - J(i-1) ) / 16
```

sipress 實作：

```rust
let recv_diff_8k = (recv_us_delta as f64 / 1_000_000.0 * 8000.0) as i64;
let rtp_diff     = rtp_ts_j.wrapping_sub(rtp_ts_i) as i64;
let d            = (recv_diff_8k - rtp_diff).unsigned_abs() as u64;

// J += (|D| - J) / 16  ← 整數近似，乘以 16 累積
let j     = jitter_x16_us.load(Relaxed);
let new_j = j.saturating_add(d).saturating_sub(j / 16);
jitter_x16_us.store(new_j, Relaxed);
```

最終輸出：`jitter_x16_us / 16 / 8 ms`（8kHz clock 單位 → ms）

### MOS 估算（ITU-T E-Model 簡化版）

```
R = 93.2 - Ie_eff - Id

Ie_eff = 0 + (95 - 0) × ppl / (ppl + 4.3)    ← G.711 Ie=0, Bpl=4.3
Id     = max(0, jitter_ms - 150) × 0.1

MOS = 1 + 0.035R + R(R-60)(100-R) × 7×10⁻⁶
```

**基礎 R 值**：G.711 PCMU 在完美條件下 R = 93.2，對應 MOS ≈ 4.4（接近「優秀」）。

**實際限制**：
- 公式對 jitter < 150ms 不計分（電信業常見標準），超過才開始扣 Id
- 掉包率扣分的曲線在 5% 左右開始明顯（ITU-T G.113 Appendix I）
- 輸出限制在 [1.0, 5.0] 避免極端值

---

## 13. core — 報告輸出 reporter.rs & html_reporter.rs

### 文字報告（reporter.rs）

支援三種輸出格式：
- `table`：帶邊框的 Unicode 框線表格（終端機顯示）
- `json`：標準 JSON，欄位名與 `FinalReport` struct 一致
- `csv`：用逗號分隔，第一行為標頭

### HTML 報告（html_reporter.rs）

內嵌所有 CSS 和 SVG，產生單一自包含 `.html` 檔案：

```
reports/YYYYMMDD_HHMMSS_report.html
```

HTML 模板使用 Rust `format!` 巨集插值，無模板引擎依賴。包含：
- SVG 環形圖（ASR）
- SVG 長條圖（PDD 百分位數）
- RTP 品質區塊（有 RTP 時才顯示）

---

## 14. core — SIP 日誌 sip_logger.rs

每次壓測產生一個帶時間戳記的 log 檔案：

```
logs/YYYYMMDD_HHMMSS_agent.sip.log
```

格式範例：

```
[00:00:01.023] ──▶ INVITE sip:29876@192.168.1.100:5060 SIP/2.0
[00:00:01.145] ◀── SIP/2.0 100 Trying
[00:00:01.243] ◀── SIP/2.0 180 Ringing
[00:00:01.501] ◀── SIP/2.0 200 OK
[00:00:01.502] ──▶ ACK sip:29876@192.168.1.100:5060 SIP/2.0
[00:00:31.502] ──▶ BYE sip:29876@192.168.1.100:5060 SIP/2.0
[00:00:31.612] ◀── SIP/2.0 200 OK
```

**注意**：SipLogger 使用 `std::fs::File` + `BufWriter`，寫入操作在 `.log_message()` 呼叫時同步進行（非 async），不會阻塞 tokio runtime 太久（檔案寫入通常極快）。高 CPS 場景下可考慮改為 async 寫入。

---

## 15. cli — 命令列介面

### args.rs（clap 參數）

```rust
#[derive(Parser)]
struct Args {
    #[arg(short = 's', long, default_value = "127.0.0.1:5060")]
    server: String,
    
    #[arg(short = 'd', long, default_value_t = 60)]
    duration: u64,  // 0 = unlimited
    
    #[arg(long, default_value = None)]
    max_calls: Option<u64>,
    // ...
}
```

### main.rs 流程

```
1. 解析 clap Args
2. 組裝 Config
3. 若 --tui：spawn TUI dashboard task
4. Engine::run(on_progress) .await
5. 輸出報告（table/json/csv）
6. 若 --html：產生 HTML 報告
```

### TUI Dashboard（tui/dashboard.rs）

使用 `ratatui` 繪製即時儀表板：
- 按 `q` 或 `Esc` 退出 TUI 顯示（引擎繼續在背景執行到完成）
- 每秒從 `on_progress` callback 收到 `StatsSnapshot` 更新畫面

---

## 16. gui — Tauri 後端 commands.rs & lib.rs

### lib.rs — 應用入口

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(AppState::default()))   // ← 必須
        .invoke_handler(tauri::generate_handler![
            start_test, stop_test, get_snapshot, get_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**關鍵：`.manage()` 必須在 `lib.rs` 中呼叫**。若 `main.rs` 有獨立的 `Builder::default()` 且沒有 `.manage()`，會導致 runtime 錯誤：`state not managed for field 'state' on command 'start_test'`。

`main.rs` 唯一的職責是呼叫 `lib::run()`：

```rust
fn main() {
    sipress_gui_lib::run();
}
```

### AppState — 跨 Command 共享狀態

```rust
pub struct AppState {
    pub snapshot: Mutex<Option<StatsSnapshot>>,  // 最新快照（前端輪詢）
    pub report:   Mutex<Option<FinalReport>>,    // 最終報告（完成後）
    pub stop_tx:  Mutex<Option<mpsc::Sender<()>>>,  // 停止信號發送端
}
```

以 `Arc<AppState>` 包裝後 `.manage()`，讓每個 command 透過 `State<'_, Arc<AppState>>` 取得。

### start_test Command

```rust
pub async fn start_test(
    state:  State<'_, Arc<AppState>>,
    config: Config,  // Tauri 自動從 JSON 反序列化
) -> Result<String, String> {
    let state = Arc::clone(&state);
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    *state.stop_tx.lock().unwrap() = Some(stop_tx);

    let on_progress: ProgressCallback = Arc::new(move |snap, _| {
        *state_cb.snapshot.lock().unwrap() = Some(snap);
    });

    tokio::spawn(async move {
        tokio::select! {
            result = engine.run(Some(on_progress)) => { /* 儲存 report */ }
            _ = stop_rx.recv() => { /* 正常停止 */ }
        }
    });

    Ok("started".to_string())  // 立即回傳，不等待引擎完成
}
```

`start_test` 立即回傳 `"started"`，引擎在背景 task 中執行。前端收到回傳後開始輪詢 `get_snapshot`。

### stop_test 的 MutexGuard 注意事項

```rust
pub async fn stop_test(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    // ✅ 取出 sender 後立即 drop guard，再 .await
    let tx = state.stop_tx.lock().unwrap().take();
    if let Some(tx) = tx {
        let _ = tx.send(()).await;  // guard 已被 drop
        Ok("stopped".to_string())
    } else {
        Err("沒有正在執行的壓測".to_string())
    }
}
```

若直接 `state.stop_tx.lock().unwrap().as_ref()?.send().await`，`MutexGuard` 會跨越 `.await`，導致 `!Send` 編譯錯誤。

### Tauri 2 Capabilities

Tauri 2 採用明確權限模型，每個 window API 都必須在 `capabilities/default.json` 宣告：

```json
{
  "permissions": [
    "core:default",
    "core:window:allow-close",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-start-dragging",
    "opener:default",
    "dialog:allow-open"
  ]
}
```

未宣告的操作（如視窗拖曳）會靜默失敗，不會有錯誤訊息，難以排查。

---

## 17. gui — 前端狀態管理 testStore.ts

### Pinia Store 架構

```typescript
export const useTestStore = defineStore('test', () => {
    const config = ref<TestConfig>({...})        // 使用者輸入
    const metrics = ref<Metrics>({...})          // 即時指標
    const series = ref<SeriesData>({...})        // 圖表資料
    const status = ref<TestStatus>('idle')       // 'idle' | 'running' | 'finished'
    const logs = ref<LogEntry[]>([])             // SIP 事件日誌
    const elapsedSec = ref(0)                    // 計時器秒數
    // ...
})
```

### Config 轉換（frontend → Rust）

前端 `TestConfig` 的欄位名與 Rust `Config` struct 的欄位名必須一致（Tauri 自動 camelCase ↔ snake_case 轉換），例如：

```typescript
// TypeScript（camelCase）
interface RustConfig {
    server_addr:          string;   // Tauri 2 預設不轉換，保持 snake_case
    max_concurrent_calls: number;
    max_total_calls:      number | null;
    duration_secs:        number;
}

function buildRustConfig(): RustConfig {
    return {
        server_addr:          config.value.server,
        duration_secs:        config.value.duration,
        max_total_calls:      config.value.caller.totalCalls > 0
                                  ? config.value.caller.totalCalls : null,
        // ...
    }
}
```

**注意**：`max_total_calls: 0` 在 Rust 端是 `Option<u64>` 的 `None`（不限），前端傳 `null` 對應 Rust `None`，傳 `0` 對應 `Some(0)`（立即停止）。因此前端必須明確將 0 轉換為 `null`。

### 輪詢機制

```typescript
async function startPolling() {
    pollingTimer = setInterval(async () => {
        const snap = await invoke<RustSnapshot | null>('get_snapshot');
        if (snap) {
            metrics.value.succeeded = snap.calls_answered;
            metrics.value.failed    = snap.calls_failed + snap.calls_timeout;
            metrics.value.queued    = Math.max(0,
                snap.calls_initiated - snap.calls_answered
                - snap.calls_failed - snap.calls_timeout);
            // ...更新圖表資料
        }
    }, 1000);
}
```

前端每 1 秒呼叫 `get_snapshot`（同步 Rust command，直接從 `AppState.snapshot` 讀取，不做任何計算），效率極高。

### 計時器與 duration=0 的處理

```typescript
clockTimer = setInterval(() => {
    elapsedSec.value++;
    // duration=0 時不自動停止，等 Rust 引擎回報完成
    if (config.value.duration > 0 && elapsedSec.value >= config.value.duration) {
        _finishTest();
    }
}, 1000);
```

當 `duration = 0` 時，前端時鐘持續計時但不觸發 `_finishTest()`。測試結束訊號由 `get_snapshot` 輪詢到 `null`（引擎結束後不再更新 snapshot）來判斷，或使用者點擊 Stop 按鈕。

---

## 18. gui — Vue 元件樹

```
App.vue
└── MainLayout.vue
    ├── TitleBar.vue          進度條 + Start/Stop 按鈕
    ├── MetricStrip.vue       8 欄即時指標（CPS/CONCUR/SUCCESS/FAILED/QUEUED/ASR/ERR%/PDD）
    └── ContentArea.vue
        ├── Sidebar.vue       左側設定面板
        ├── ChartPanel.vue    中間折線圖（多個標籤頁）
        ├── RightPanel.vue    右側詳細資訊
        │   ├── CallStatus.vue    通話狀態分佈
        │   ├── RtpQuality.vue    MOS/掉包/Jitter
        │   ├── ResponseCodes.vue 回應碼統計
        │   └── SipFlow.vue       最後通話時序圖
        └── LogPanel.vue      底部 SIP 日誌
```

### Sidebar.vue 關鍵設計

- `duration` 欄位 `min="0"`，設為 0 時顯示「不限時間」提示
- `totalCalls` 欄位設為 0 時顯示「不限（依測試時長）」提示
- 音檔選擇器使用 `@tauri-apps/plugin-dialog` 的 `open()` 函數（非 `<input type="file">`，因為 Tauri WebView 的 file input 在某些平台需要特殊 capability）

### ChartPanel.vue — 折線圖

使用 Chart.js / Vue-ChartJS，每次從 store 的 `series` 讀取最新資料點更新圖表。圖表有最大資料點數限制（通常 300 點），超過後自動移除最舊的點。

---

## 19. 建置系統 build.ps1 / build.sh

### 輸出路徑

```powershell
# Windows（build.ps1）
$GUI_RELEASE = "target\release"   # workspace root 下的 target
```

```bash
# Linux/macOS（build.sh）
GUI_TARGET_DIR="target/release"   # workspace root 下的 target
```

**陷阱**：若誤用 `gui\src-tauri\target\release`，Tauri 跨 workspace 編譯時找不到產出物。

### GUI Bundle 輸出路徑

Tauri 打包產出的安裝檔（.msi/.deb/.dmg）位於：

```
target/release/bundle/msi/     ← Windows MSI 安裝版
target/release/bundle/nsis/    ← Windows NSIS 安裝版
target/release/bundle/deb/     ← Linux .deb 安裝版
target/release/bundle/appimage/ ← Linux AppImage 免安裝版
target/release/bundle/dmg/     ← macOS .dmg 安裝版
```

build script 從這些路徑複製到 `dist/`。

---

## 20. 關鍵設計決策與注意事項

### A. 為什麼用 UDP 而非 TCP for SIP？

預設使用 UDP，因為大多數中國電信軟交換機（SoftX3000、S8500 等）預設接受 UDP SIP。TCP 雖然更可靠，但 TCP 連線管理增加了壓測工具的複雜度，且在高 CPS 場景下 TCP 的 SYN 握手開銷更明顯。

### B. 為什麼不實作 SIP 認證（401/407）？

sipress 設計為純壓測工具，假設測試 SIP 伺服器端已白名單測試來源 IP，或對測試 Trunk 停用認證。實作 MD5-AES Digest 認證會大幅增加複雜度，且多數壓測場景不需要。

### C. AtomicU64 vs Mutex 的選擇

- 頻繁讀寫的計數器（calls_initiated 等）→ `AtomicU64`（無鎖，最快）
- 需要條件讀寫或複合操作的狀態（snapshot、stop_tx）→ `Mutex`
- 需要可變借用且不能 Clone 的資料（Histogram）→ `Mutex`

### D. tokio::Mutex vs std::sync::Mutex

- `tokio::Mutex`：跨 await 持有 lock 時使用（engine 中的 `dialogs`、`rtp_sessions`）
- `std::sync::Mutex`：不跨 await 的快速操作（commands.rs 中的 `AppState`）

`std::sync::Mutex` 性能更好，但 guard 不能跨 `.await`。誤用會導致 `Future is not Send` 編譯錯誤。

### E. 為什麼不使用 SRTP / TLS？

sipress 目前不支援 SRTP（加密 RTP）或 SIP over TLS，原因：
1. 企業內網壓測不需要加密
2. 加密解密增加 CPU 開銷，影響最高 CPS 測量
3. TLS 憑證管理增加部署複雜度

若有需求，TLS 支援可透過在 `Transport::Tls` 分支加入 `tokio-rustls` 實作。

### F. 大量並發通話的記憶體估算

每個 `Dialog` 約佔 200-400 bytes（包含 `String` 欄位）。
10,000 並發通話 ≈ 4 MB。在現代機器上不成問題。

RTP 每個 Session 有兩個 tokio task stack（預設 2MB 每 task），但 tokio 使用動態 stack，實際開銷遠小於此。

### G. SIP Dialog 的 HashMap 競爭

引擎主控迴圈中有三個地方鎖定 `dialogs`：
1. 事件處理（①）
2. 逾時掃描（②）
3. 發起新通話（③）

這三步是**順序執行**（不是並發），每次都 `lock().await` 然後立即 drop。接收 Task 透過 channel 傳遞事件，不直接存取 `dialogs`，所以不存在真正的競爭。

### H. 壓測工具本身的性能上限

在 3GHz 單核的機器上，sipress 能穩定達到約：
- 純 SIP 信令：~2000 CPS
- SIP + RTP（靜音）：~500 CPS
- SIP + RTP（音檔）：~200 CPS（受音檔解碼速度限制）

瓶頸主要在：
1. 主控迴圈的 500µs 睡眠（2000 CPS 上限）
2. UDP 封包的系統呼叫開銷
3. RTP 音訊解碼（啟用音檔時）

---

## 附錄：常見錯誤與排查

| 錯誤訊息 | 原因 | 解法 |
|---------|------|------|
| `state not managed for field 'state'` | `main.rs` 有獨立 Builder 不含 `.manage()` | 讓 `main.rs` 只呼叫 `lib::run()` |
| `MutexGuard is not Send` | std Mutex guard 跨 `.await` | 在 `.await` 前 drop guard |
| `Future is not Send` | tokio::spawn 的 async block 持有非 Send 型態 | 改用 tokio::Mutex 或縮短 lock 範圍 |
| `無法找到可用的 RTP port` | 系統 RTP port 耗盡（>4000 個） | 降低並發數或增加 rtp_base_port 範圍 |
| `SIP log 建立失敗` | logs/ 目錄不存在或無寫入權限 | 程式已 fallback 到 temp 目錄，檢查終端機輸出 |
| GUI 顯示空白 | Tauri WebView 設定問題 | 刪除 `%APPDATA%\com.leozh.sipress` 重啟 |
| 視窗無法拖曳/最小化 | Tauri capabilities 缺少 window 權限 | 確認 `capabilities/default.json` 有 `core:window:allow-*` |
| 所有通話 408 Timeout | 伺服器不可達（這是正確行為） | 確認 SIP 伺服器地址正確且防火牆開放 |

---

如有疑問或發現文件有誤，歡迎在 Repository 開 Issue。
