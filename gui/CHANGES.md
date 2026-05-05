# sipress-gui 修正與優化說明

## Rust 後端（src-tauri/）

### `lib.rs` — 重寫（原為 greet() stub）
- 移除原本無用的 `greet()` command
- 透過 `.manage(Arc::new(AppState::default()))` 正確注入共享狀態
- 改用 `try_init()` 避免 `main.rs` 與 `lib.rs` 雙重初始化 panic
- 同時服務 Desktop binary（main.rs）與 Mobile（lib entry point）

### `main.rs` — 重構
- 移除重複的 `invoke_handler` / `manage` 呼叫
- 改為只呼叫 `sipress_gui_lib::run()`，讓 lib.rs 統一管理

### `commands.rs` — 修正 + 新增
| 問題 | 修正方式 |
|------|---------|
| `std::Mutex` 在 async context 中阻塞 tokio executor | 改用 `tokio::sync::RwLock` |
| stop 機制只有 mpsc channel，engine 感知不到 | 新增 `AtomicBool stop_flag`，engine task 每 100ms 輪詢 |
| `stopTest` 沒有等 engine task 結束 | 新增 `JoinHandle`，`stop_test` 用 `timeout(3s)` await |
| `get_snapshot` / `get_report` 用 write lock | 改用 `read()` 鎖，減少爭用 |
| ProgressCallback 中 lock 可能死鎖 | 改用 `try_write()`，失敗則跳過 |
| 缺少狀態查詢 command | 新增 `get_status` → 前端輪詢時判斷 idle/running/done/error |

### `Cargo.toml` — 更新
- 補齊 `tokio = { features = ["full"] }`
- 新增 `tauri-plugin-dialog = "2"`（Sidebar 音訊選擇器使用）
- `tracing-subscriber` 加入 `features = ["env-filter"]`

---

## 前端（src/）

### `stores/testStore.ts` — 大幅擴充

**新增型別：**
- `RtpMetrics`：MOS / 掉包率 / Jitter / 封包統計（對應 README §RTP 聲音品質指標）
- `FinalReport`：對應 Rust `FinalReport` struct（含 RTP、延遲分位數、錯誤碼）
- `Metrics` 新增 `ccr / timeout / initiated / answered / completed`

**Tauri 整合層：**
- 新增 `invoke()` shim：Tauri 環境用真實 `@tauri-apps/api/core`，純瀏覽器自動 fallback 模擬模式
- `startTest()`：Tauri 環境下組裝 `Config` 呼叫 `start_test` command，啟動輪詢
- `stopTest()`：Tauri 環境呼叫 `stop_test` command，停止輪詢
- `_pollTauri()`：每秒輪詢 `get_status` / `get_snapshot`，完成後拉取 `get_report`
- `applySnapshot()`：將後端 `StatsSnapshot` 映射到前端所有指標

**模擬器（非 Tauri fallback）：**
- 模擬 RTP 指標（含 E-Model MOS 計算公式，與 README 一致）
- 新增 CCR 模擬（略低於 ASR）
- `exportJson` 優先輸出真實 `FinalReport`
- `exportCsv` 加入 ccr / mos 欄位

**CLI command 產生器更新：**
- 加入 `--rtp` / `--rtp-port` / `--call-duration` / `--invite-timeout` 參數

### `MetricStrip.vue`
- **新增 CCR 格子**（grid 從 6 欄改為 7 欄）
- CCR 顏色：≥80% 優 / ≥65% 普通 / <65% 差

### `ChartPanel.vue`
- **新增 CCR tab**（對應 README CCR 指標）
- **新增 MOS tab**（紫色 `#a855f7`，y 軸固定 1~5）
  - MOS ≥4.0 / ≥3.0 參考線（虛線 + 文字標記）
  - RTP 未啟用時顯示「未啟用 RTP」提示文字
- 修正 Light theme 下 grid / label 顏色（原本固定 rgba(255,255,255,...) 在亮色下不可見）
- 右上角顯示即時 MOS 數值 badge（含品質評級色點）

### `RightPanel.vue`
- **新增「RTP 聲音品質」區塊**：
  - MOS 進度條 + 評級（優/普通/差）
  - 掉包率進度條（<1% 優，<3% 普通，≥3% 差）
  - Jitter 進度條（<30ms 優，<60ms 普通，≥60ms 差）
  - 封包統計（Sent / Recv / OOO 亂序）
  - 未啟用 RTP 時顯示提示文字與 `--rtp` / `--audio` 說明

### `Sidebar.vue`
- **新增「通話控制」欄位**：`--call-duration` / `--invite-timeout`
- **RTP 音訊 section 重構**：
  - inline toggle switch 控制 `enableRtp`
  - 展開後顯示 `--rtp-port` 與音訊檔案選擇
  - 收合但已啟用時顯示小提示 badge
- `pickAudioFile()`：Tauri 環境使用 `@tauri-apps/plugin-dialog` 真實檔案對話框
- CLI command 即時反映所有新參數
