/// Tauri commands：前端 ↔ core 橋接層
use sipress_core::{
    config::Config,
    engine::{Engine, ProgressCallback},
    html_reporter::HtmlReporter,
    stats::{FinalReport, StatsSnapshot},
};
use std::sync::{Arc, Mutex};
use tauri::State;
use tokio::sync::mpsc;

// ── 全域狀態 ─────────────────────────────────────────────────────

/// 跨 command 共享的執行狀態
pub struct AppState {
    pub snapshot:   Mutex<Option<StatsSnapshot>>,
    pub report:     Mutex<Option<FinalReport>>,
    pub stop_tx:    Mutex<Option<mpsc::Sender<()>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(None),
            report:   Mutex::new(None),
            stop_tx:  Mutex::new(None),
        }
    }
}

// ── Tauri Commands ────────────────────────────────────────────────

/// 啟動壓測
/// 前端呼叫：await invoke('start_test', { config: {...} })
#[tauri::command]
pub async fn start_test(
    state:  State<'_, Arc<AppState>>,
    config: Config,
) -> Result<String, String> {
    let state = Arc::clone(&state);

    // 清除舊結果
    *state.snapshot.lock().unwrap() = None;
    *state.report.lock().unwrap()   = None;

    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    *state.stop_tx.lock().unwrap() = Some(stop_tx);

    let state_cb = Arc::clone(&state);
    let on_progress: ProgressCallback = Arc::new(move |snap: StatsSnapshot, _progress: f64| {
        *state_cb.snapshot.lock().unwrap() = Some(snap);
    });

    tokio::spawn(async move {
        let engine = Engine::new(config);
        tokio::select! {
            result = engine.run(Some(on_progress)) => {
                match result {
                    Ok(report) => {
                        *state.report.lock().unwrap() = Some(report);
                    }
                    Err(e) => {
                        tracing::error!("Engine 錯誤: {}", e);
                    }
                }
            }
            _ = stop_rx.recv() => {
                tracing::info!("壓測被手動停止");
            }
        }
    });

    Ok("started".to_string())
}

/// 手動停止壓測
#[tauri::command]
pub async fn stop_test(
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    // Take the sender out BEFORE any await so MutexGuard is not held across threads
    let tx = state.stop_tx.lock().unwrap().take();
    if let Some(tx) = tx {
        let _ = tx.send(()).await;
        Ok("stopped".to_string())
    } else {
        Err("沒有正在執行的壓測".to_string())
    }
}

/// 取得即時快照（前端每秒輪詢）
#[tauri::command]
pub fn get_snapshot(
    state: State<'_, Arc<AppState>>,
) -> Option<StatsSnapshot> {
    state.snapshot.lock().unwrap().clone()
}

/// 取得最終報告（壓測完成後）
#[tauri::command]
pub fn get_report(
    state: State<'_, Arc<AppState>>,
) -> Option<FinalReport> {
    state.report.lock().unwrap().clone()
}

/// 產生 HTML 報告字串（前端負責下載或開啟）
#[tauri::command]
pub fn get_html_report(
    state:       State<'_, Arc<AppState>>,
    server_addr: String,
    timestamp:   String,
) -> Result<String, String> {
    let report = state.report.lock().unwrap().clone()
        .ok_or_else(|| "尚無測試結果，請先完成或停止一次壓測".to_string())?;
    Ok(HtmlReporter::render(&report, &timestamp, &server_addr))
}
