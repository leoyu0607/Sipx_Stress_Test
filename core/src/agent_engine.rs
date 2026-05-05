/// 座席端壓測引擎
///
/// 每個帳號各自一個 task，行為：
///   1. 用獨立 UDP socket REGISTER（含 401 Digest 認證重送）
///   2. 在 Expires/2 時自動 re-REGISTER 維持註冊
///   3. 監聽 INVITE → 自動 100 Trying → 200 OK + SDP
///   4. 監聽 ACK / RE-INVITE / BYE，回應對應訊息
///   5. 結束時送 REGISTER Expires=0 解除註冊
///
/// 設計原則：簡單、可觀察。用單一 socket 收發，避免多 socket 同步問題。
use crate::config::{AgentAccount, Config};
use crate::engine::ProgressCallback;
use crate::sip::{
    register::{DigestChallenge, RegisterMessage},
    SipMessage, SipResponse,
};
use crate::sip_logger::{Direction, SipLogger, SipRole};
use crate::stats::{DetailedStats, FinalReport, LiveStats};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time;

pub struct AgentEngine {
    config: Config,
}

impl AgentEngine {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn run(
        &self,
        on_progress: Option<ProgressCallback>,
    ) -> Result<FinalReport> {
        let cfg     = self.config.clone();
        let live    = Arc::new(LiveStats::default());
        let detail  = Arc::new(DetailedStats::default());
        let start   = Instant::now();

        // SIP log
        let sip_log = Arc::new(
            SipLogger::new(&cfg.logs_dir, SipRole::Agent)
                .unwrap_or_else(|e| {
                    eprintln!("[sipress agent] 警告：無法建立 SIP log：{}", e);
                    let tmp = std::env::temp_dir();
                    SipLogger::new(&tmp.to_string_lossy(), SipRole::Agent)
                        .expect("無法在系統暫存目錄建立 SIP log")
                }),
        );
        eprintln!("[sipress agent] SIP log → {}", sip_log.path.display());

        let server: SocketAddr = cfg.server_addr.parse()
            .with_context(|| format!("無效的 server 位址: {}", cfg.server_addr))?;

        if cfg.agent_accounts.is_empty() {
            anyhow::bail!("座席模式需要至少一個 agent account");
        }

        // ── 進度回報 task ──
        if let Some(cb) = on_progress {
            let live2     = Arc::clone(&live);
            let duration  = cfg.duration_secs as f64;
            let unlimited = cfg.duration_secs == 0;
            tokio::spawn(async move {
                let mut interval = time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    let elapsed = start.elapsed().as_secs_f64();
                    let progress = if unlimited { 0.0 } else { (elapsed / duration).min(1.0) };
                    cb(live2.snapshot(), progress);
                    if !unlimited && elapsed >= duration { break; }
                }
            });
        }

        // ── 為每個帳號 spawn 一個 runner ──
        let stop_flag = Arc::new(tokio::sync::Notify::new());
        let mut handles = Vec::new();
        for acc in &cfg.agent_accounts {
            let acc       = acc.clone();
            let cfg       = cfg.clone();
            let live      = Arc::clone(&live);
            let log       = Arc::clone(&sip_log);
            let stop      = Arc::clone(&stop_flag);
            let h = tokio::spawn(async move {
                if let Err(e) = account_runner(acc, server, cfg, live, log, stop).await {
                    eprintln!("[sipress agent] runner 結束: {}", e);
                }
            });
            handles.push(h);
        }

        // ── 等待測試時間到 ──
        let unlimited_time = cfg.duration_secs == 0;
        if unlimited_time {
            // 永遠等待（透過外部 stop 訊號才會結束）；此處用很長的睡眠模擬
            time::sleep(Duration::from_secs(u64::MAX / 2)).await;
        } else {
            time::sleep(cfg.duration()).await;
        }

        // 通知所有 runner 結束（會送 REGISTER Expires=0）
        stop_flag.notify_waiters();
        // 給 runner 1 秒時間 deregister
        time::sleep(Duration::from_secs(1)).await;
        for h in handles { h.abort(); }

        // ── 產生最終報告 ──
        let snap = live.snapshot();
        let elapsed = start.elapsed().as_secs_f64();

        sip_log.log_summary(&format!(
            "[座席] 來電={} 接聽={} 完成={} 失敗={} 時長={:.1}s",
            snap.calls_initiated, snap.calls_answered, snap.calls_completed,
            snap.calls_failed, elapsed,
        ));

        let us_to_ms = |h: &hdrhistogram::Histogram<u64>, q: f64| {
            h.value_at_quantile(q) as f64 / 1000.0
        };
        let (pdd_p50, pdd_p95, pdd_p99, pdd_max,
             setup_p50, setup_p95, setup_p99, setup_max,
             acd_secs) = {
            let pdd_h   = detail.pdd_hist.lock().unwrap();
            let setup_h = detail.setup_hist.lock().unwrap();
            let dur_h   = detail.dur_hist.lock().unwrap();
            let acd = if snap.calls_completed > 0 { dur_h.mean() / 1000.0 } else { 0.0 };
            (
                us_to_ms(&pdd_h, 0.50), us_to_ms(&pdd_h, 0.95),
                us_to_ms(&pdd_h, 0.99), pdd_h.max() as f64 / 1000.0,
                us_to_ms(&setup_h, 0.50), us_to_ms(&setup_h, 0.95),
                us_to_ms(&setup_h, 0.99), setup_h.max() as f64 / 1000.0,
                acd,
            )
        };

        Ok(FinalReport {
            calls_initiated: snap.calls_initiated,
            calls_answered:  snap.calls_answered,
            calls_completed: snap.calls_completed,
            calls_failed:    snap.calls_failed,
            calls_timeout:   snap.calls_timeout,
            duration_secs:   elapsed,
            asr:             snap.asr,
            ccr: if snap.calls_initiated > 0 {
                snap.calls_completed as f64 / snap.calls_initiated as f64 * 100.0
            } else { 0.0 },
            actual_cps: snap.calls_initiated as f64 / elapsed.max(0.001),
            pdd_p50_ms: pdd_p50, pdd_p95_ms: pdd_p95, pdd_p99_ms: pdd_p99, pdd_max_ms: pdd_max,
            setup_p50_ms: setup_p50, setup_p95_ms: setup_p95,
            setup_p99_ms: setup_p99, setup_max_ms: setup_max,
            acd_secs,
            fail_4xx: detail.fail_4xx.load(std::sync::atomic::Ordering::Relaxed),
            fail_5xx: detail.fail_5xx.load(std::sync::atomic::Ordering::Relaxed),
            fail_6xx: detail.fail_6xx.load(std::sync::atomic::Ordering::Relaxed),
            mos: None, loss_rate_pct: None, jitter_ms: None,
            rtp_sent: None, rtp_recv: None, rtp_out_of_order: None,
        })
    }
}

// ─── 單一帳號 runner ──────────────────────────────────────────────

struct DialogCtx {
    /// 對應 INVITE 的原始訊息（用於建構 200 OK / BYE 200 OK）
    invite_raw:   String,
    /// 我方 To tag（送 200 OK 時加上）
    local_to_tag: String,
    answered_at:  Option<Instant>,
}

async fn account_runner(
    account: AgentAccount,
    server:  SocketAddr,
    cfg:     Config,
    live:    Arc<LiveStats>,
    log:     Arc<SipLogger>,
    stop:    Arc<tokio::sync::Notify>,
) -> Result<()> {
    // 建立持久 socket
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(server).await?;
    let local_addr = sock.local_addr()?.to_string();
    let local_ip = local_addr.split(':').next().unwrap_or("0.0.0.0").to_string();

    let domain = if !account.domain.is_empty() {
        account.domain.clone()
    } else {
        server.ip().to_string()
    };

    // 共用的 from_tag / call_id（同一個 REGISTER 對話用同組）
    let reg_from_tag = SipMessage::new_tag();
    let reg_call_id  = SipMessage::new_call_id(&domain);
    let reg_state    = Arc::new(Mutex::new(RegState {
        cseq:      0,
        challenge: None,
    }));

    // ── 第一次 REGISTER ──
    let initial_expires = 600u32;
    let mut current_expires = initial_expires;
    let server_addr_str = cfg.server_addr.clone();
    let transport_str = match cfg.transport {
        crate::config::Transport::Udp => "UDP",
        crate::config::Transport::Tcp => "TCP",
    };

    // 註冊狀態：用 channel 等首次註冊結果
    let dialogs: Arc<Mutex<HashMap<String, DialogCtx>>> = Arc::new(Mutex::new(HashMap::new()));

    log.log_event(&account.extension, "開始 REGISTER");

    if let Err(e) = send_register(
        &sock, &log, &server_addr_str, &domain, &local_addr,
        &account, &reg_from_tag, &reg_call_id, transport_str,
        initial_expires, None, &mut *reg_state.lock().await,
    ).await {
        log.log_event(&account.extension, &format!("初始 REGISTER 送出失敗: {}", e));
        return Err(e);
    }

    // 主迴圈：邊聽邊處理 stop / re-register
    let mut buf = vec![0u8; 65536];
    let mut last_register_at = Instant::now();
    let mut registered = false;

    loop {
        // 計算下一次 re-register 的時點
        let refresh_at = last_register_at + Duration::from_secs((current_expires as u64 / 2).max(60));

        tokio::select! {
            // 收到 SIP 訊息
            res = sock.recv(&mut buf) => {
                let n = match res {
                    Ok(n) => n,
                    Err(e) => { log.log_event(&account.extension, &format!("recv 失敗: {}", e)); break; }
                };
                let raw = String::from_utf8_lossy(&buf[..n]).into_owned();
                log.log_message(Direction::Recv, &raw, &server.to_string());

                if raw.starts_with("SIP/2.0") {
                    handle_response(
                        &raw, &sock, &log, &cfg, &server_addr_str, &domain, &local_addr,
                        &account, &reg_from_tag, &reg_call_id, transport_str,
                        &mut current_expires, &mut last_register_at, &mut registered,
                        Arc::clone(&reg_state),
                    ).await;
                } else {
                    handle_request(
                        &raw, &sock, &log, &local_addr, &local_ip,
                        &account, &cfg, &live, Arc::clone(&dialogs),
                    ).await;
                }
            }

            // re-register 計時
            _ = time::sleep_until(tokio::time::Instant::from_std(refresh_at)) => {
                log.log_event(&account.extension, "re-REGISTER（刷新）");
                last_register_at = Instant::now();
                let mut st = reg_state.lock().await;
                st.cseq = st.cseq.wrapping_add(1);
                let cseq = st.cseq;
                let challenge = st.challenge.clone();
                drop(st);
                let auth = challenge.as_ref().map(|c| {
                    c.build_authorization(&account.username, &account.password, "REGISTER",
                                          &format!("sip:{}", server_addr_str))
                });
                let req = RegisterMessage::build(
                    &account.username, &domain, &server_addr_str, &local_addr,
                    cseq, &SipMessage::new_branch(), &reg_from_tag, &reg_call_id,
                    transport_str, current_expires, auth.as_deref(),
                );
                log.log_message(Direction::Send, &req, &server.to_string());
                let _ = sock.send(req.as_bytes()).await;
            }

            // 收到外部停止訊號 → 解除註冊
            _ = stop.notified() => {
                log.log_event(&account.extension, "收到停止訊號，發送 REGISTER Expires=0");
                let mut st = reg_state.lock().await;
                st.cseq = st.cseq.wrapping_add(1);
                let cseq = st.cseq;
                let challenge = st.challenge.clone();
                drop(st);
                let auth = challenge.as_ref().map(|c| {
                    c.build_authorization(&account.username, &account.password, "REGISTER",
                                          &format!("sip:{}", server_addr_str))
                });
                let req = RegisterMessage::build(
                    &account.username, &domain, &server_addr_str, &local_addr,
                    cseq, &SipMessage::new_branch(), &reg_from_tag, &reg_call_id,
                    transport_str, 0, auth.as_deref(),
                );
                log.log_message(Direction::Send, &req, &server.to_string());
                let _ = sock.send(req.as_bytes()).await;
                // 給網路一點時間
                let _ = time::timeout(Duration::from_millis(300), sock.recv(&mut buf)).await;
                break;
            }
        }
    }

    Ok(())
}

// ─── REGISTER 狀態 ────────────────────────────────────────────────

struct RegState {
    cseq:      u32,
    /// 收到 401 後快取下來的 challenge（用於 re-register / deregister 重複利用）
    challenge: Option<DigestChallenge>,
}

#[allow(clippy::too_many_arguments)]
async fn send_register(
    sock:        &UdpSocket,
    log:         &SipLogger,
    server_addr: &str,
    domain:      &str,
    local_addr:  &str,
    account:     &AgentAccount,
    from_tag:    &str,
    call_id:     &str,
    transport:   &str,
    expires:     u32,
    auth:        Option<&str>,
    state:       &mut RegState,
) -> Result<()> {
    state.cseq = state.cseq.wrapping_add(1);
    let req = RegisterMessage::build(
        &account.username, domain, server_addr, local_addr,
        state.cseq, &SipMessage::new_branch(), from_tag, call_id,
        transport, expires, auth,
    );
    log.log_message(Direction::Send, &req, server_addr);
    sock.send(req.as_bytes()).await?;
    Ok(())
}

// ─── 收到 SIP 回應的處理 ──────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn handle_response(
    raw:                &str,
    sock:               &UdpSocket,
    log:                &SipLogger,
    _cfg:               &Config,
    server_addr_str:    &str,
    domain:             &str,
    local_addr:         &str,
    account:            &AgentAccount,
    reg_from_tag:       &str,
    reg_call_id:        &str,
    transport_str:      &str,
    current_expires:    &mut u32,
    last_register_at:   &mut Instant,
    registered:         &mut bool,
    reg_state:          Arc<Mutex<RegState>>,
) {
    let code = SipResponse::status_code(raw).unwrap_or(0);
    let method = SipResponse::cseq_method(raw);

    // 我們只在意 REGISTER 的回應（其他例如 BYE 200 OK 是被動）
    if method.as_deref() != Some("REGISTER") {
        return;
    }

    match code {
        200 => {
            *registered = true;
            *last_register_at = Instant::now();
            log.log_event(&account.extension, "註冊成功");
            // 回傳的 Expires 可能不同
            if let Some(exp) = parse_expires(raw) {
                *current_expires = exp;
            }
        }
        401 | 407 => {
            // 解析 challenge → 帶 auth 重送
            if let Some(chal) = DigestChallenge::parse(raw) {
                let auth = chal.build_authorization(
                    &account.username, &account.password, "REGISTER",
                    &format!("sip:{}", server_addr_str),
                );
                {
                    let mut st = reg_state.lock().await;
                    st.challenge = Some(chal);
                    if let Err(e) = send_register(
                        sock, log, server_addr_str, domain, local_addr,
                        account, reg_from_tag, reg_call_id, transport_str,
                        *current_expires, Some(&auth), &mut *st,
                    ).await {
                        log.log_event(&account.extension, &format!("認證重送失敗: {}", e));
                    }
                }
            } else {
                log.log_event(&account.extension, "無法解析 Digest challenge");
            }
        }
        _ => {
            log.log_event(&account.extension, &format!("REGISTER 回應 SIP {}", code));
            *registered = false;
        }
    }
}

// ─── 收到 SIP 請求的處理（INVITE / BYE / ACK / RE-INVITE）──────────

async fn handle_request(
    raw:        &str,
    sock:       &UdpSocket,
    log:        &SipLogger,
    local_addr: &str,
    local_ip:   &str,
    account:    &AgentAccount,
    _cfg:       &Config,
    live:       &LiveStats,
    dialogs:    Arc<Mutex<HashMap<String, DialogCtx>>>,
) {
    let method = raw.lines().next()
        .and_then(|l| l.split_whitespace().next())
        .map(|s| s.to_uppercase())
        .unwrap_or_default();

    let call_id = match raw.lines()
        .find(|l| l.to_lowercase().starts_with("call-id:"))
        .and_then(|l| l.splitn(2, ':').nth(1))
        .map(|s| s.trim().to_string())
    { Some(c) => c, None => return };

    match method.as_str() {
        "INVITE" => {
            // 判斷是新通話還是 RE-INVITE（dialog 已存在）
            let mut dlgs = dialogs.lock().await;
            let existing = dlgs.contains_key(&call_id);
            if existing {
                // RE-INVITE：直接回 200 OK + SDP
                let port = pick_dummy_rtp_port(local_ip);
                let ok = SipMessage::ok_for_server_reinvite(raw, local_addr, port);
                log.log_message(Direction::Send, &ok, "server");
                let _ = sock.send(ok.as_bytes()).await;
                log.log_event(&account.extension, &format!("[{}] 回應 RE-INVITE", short(&call_id)));
            } else {
                // 新通話 → 100 Trying → 200 OK + SDP
                live.on_invite();

                let local_to_tag = SipMessage::new_tag();
                // 100 Trying
                let trying = build_response_no_body(raw, "100 Trying", "");
                log.log_message(Direction::Send, &trying, "server");
                let _ = sock.send(trying.as_bytes()).await;

                // 200 OK + SDP（用我們的本機 IP/port）
                let port = pick_dummy_rtp_port(local_ip);
                let ok = build_response_with_sdp(raw, "200 OK", &local_to_tag, local_addr, port);
                log.log_message(Direction::Send, &ok, "server");
                let _ = sock.send(ok.as_bytes()).await;

                live.on_answered();

                let ctx = DialogCtx {
                    invite_raw:   raw.to_string(),
                    local_to_tag,
                    answered_at:  Some(Instant::now()),
                };
                dlgs.insert(call_id.clone(), ctx);
                log.log_event(&account.extension, &format!("[{}] 接聽來電", short(&call_id)));
            }
        }
        "ACK" => {
            // 三方握手結束，無需回應
        }
        "BYE" => {
            // 對方掛斷
            let ok = SipMessage::ok_for_server_bye(raw);
            log.log_message(Direction::Send, &ok, "server");
            let _ = sock.send(ok.as_bytes()).await;

            let mut dlgs = dialogs.lock().await;
            if let Some(_ctx) = dlgs.remove(&call_id) {
                live.on_completed();
                log.log_event(&account.extension, &format!("[{}] 通話結束", short(&call_id)));
            }
        }
        "CANCEL" => {
            // 來電在接通前被取消：先回 200 OK for CANCEL，再對 INVITE 回 487
            let ok = SipMessage::ok_for_server_bye(raw); // 結構相同：echo headers + Content-Length: 0
            log.log_message(Direction::Send, &ok, "server");
            let _ = sock.send(ok.as_bytes()).await;
            // 對 INVITE 回 487（用同組 dialog 標頭）
            let mut dlgs = dialogs.lock().await;
            if let Some(ctx) = dlgs.get(&call_id) {
                let resp = build_response_no_body(&ctx.invite_raw, "487 Request Terminated", &ctx.local_to_tag);
                log.log_message(Direction::Send, &resp, "server");
                let _ = sock.send(resp.as_bytes()).await;
                dlgs.remove(&call_id);
                live.on_failed();
            }
        }
        "OPTIONS" => {
            // 健康檢查：回 200 OK
            let ok = build_response_no_body(raw, "200 OK", "");
            log.log_message(Direction::Send, &ok, "server");
            let _ = sock.send(ok.as_bytes()).await;
        }
        _ => {
            log.log_event(&account.extension, &format!("忽略未知請求: {}", method));
        }
    }
}

// ─── 共用：建構回應訊息 ─────────────────────────────────────────────

/// 建構不含 body 的 SIP 回應（如 100 Trying / 200 OK for BYE / 487 等）
fn build_response_no_body(raw_request: &str, status_line: &str, extra_to_tag: &str) -> String {
    let (via, from, to, call_id, cseq) = extract_request_headers_for_response(raw_request);
    let to_with_tag = inject_to_tag_if_missing(&to, extra_to_tag);
    format!(
        "SIP/2.0 {status}\r\n\
         {via}\r\n\
         {from}\r\n\
         {to}\r\n\
         {call_id}\r\n\
         {cseq}\r\n\
         Content-Length: 0\r\n\
         \r\n",
        status = status_line, via = via, from = from,
        to = to_with_tag, call_id = call_id, cseq = cseq,
    )
}

/// 建構含 SDP 的 200 OK（給接聽 INVITE 用）
fn build_response_with_sdp(raw_request: &str, status_line: &str, to_tag: &str,
                            local_addr: &str, rtp_port: u16) -> String {
    let (via, from, to, call_id, cseq) = extract_request_headers_for_response(raw_request);
    let to_with_tag = inject_to_tag_if_missing(&to, to_tag);
    let ip = local_addr.split(':').next().unwrap_or(local_addr);
    let sdp = format!(
        "v=0\r\n\
         o=sipress 2000 2000 IN IP4 {ip}\r\n\
         s=sipress\r\n\
         c=IN IP4 {ip}\r\n\
         t=0 0\r\n\
         m=audio {port} RTP/AVP 8\r\n\
         a=rtpmap:8 PCMA/8000\r\n\
         a=ptime:20\r\n\
         a=sendrecv\r\n",
        ip = ip, port = rtp_port,
    );
    let sdp_len = sdp.len();
    let user_part = from
        .split_once("sip:")
        .map(|(_, s)| s.split('@').next().unwrap_or("agent"))
        .unwrap_or("agent");
    format!(
        "SIP/2.0 {status}\r\n\
         {via}\r\n\
         {from}\r\n\
         {to}\r\n\
         {call_id}\r\n\
         {cseq}\r\n\
         Contact: <sip:{user}@{local};transport=udp>\r\n\
         Content-Type: application/sdp\r\n\
         Content-Length: {sdp_len}\r\n\
         \r\n\
         {sdp}",
        status = status_line, via = via, from = from,
        to = to_with_tag, call_id = call_id, cseq = cseq,
        user = user_part, local = local_addr,
        sdp_len = sdp_len, sdp = sdp,
    )
}

fn extract_request_headers_for_response(raw: &str) -> (String, String, String, String, String) {
    let mut vias = Vec::<String>::new();
    let (mut from, mut to, mut call_id, mut cseq) =
        (String::new(), String::new(), String::new(), String::new());
    for line in raw.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("via:") || lower.starts_with("v:") {
            vias.push(line.to_string());
        } else if from.is_empty() && (lower.starts_with("from:") || lower.starts_with("f:")) {
            from = line.to_string();
        } else if to.is_empty() && (lower.starts_with("to:") || lower.starts_with("t:")) {
            to = line.to_string();
        } else if call_id.is_empty() && (lower.starts_with("call-id:") || lower.starts_with("i:")) {
            call_id = line.to_string();
        } else if cseq.is_empty() && lower.starts_with("cseq:") {
            cseq = line.to_string();
        }
    }
    (vias.join("\r\n"), from, to, call_id, cseq)
}

fn inject_to_tag_if_missing(to_line: &str, tag: &str) -> String {
    if tag.is_empty() {
        return to_line.to_string();
    }
    if to_line.to_lowercase().contains(";tag=") {
        to_line.to_string()
    } else {
        format!("{};tag={}", to_line.trim_end(), tag)
    }
}

fn parse_expires(raw: &str) -> Option<u32> {
    for line in raw.lines() {
        if line.to_lowercase().starts_with("expires:") {
            if let Some(val) = line.splitn(2, ':').nth(1) {
                if let Ok(n) = val.trim().parse::<u32>() {
                    return Some(n);
                }
            }
        }
    }
    for line in raw.lines() {
        if line.to_lowercase().starts_with("contact:") {
            if let Some(idx) = line.to_lowercase().find("expires=") {
                let rest = &line[idx + 8..];
                let val: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = val.parse::<u32>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// 隨機產生一個 RTP port（給 SDP 宣告用，本版不真的綁定）
/// 之後 Phase 3 才會做真實 RTP 收發
fn pick_dummy_rtp_port(_local_ip: &str) -> u16 {
    use rand::Rng;
    // 16000~32000 之間隨機（與 caller 模式類似的範圍）
    let p = rand::thread_rng().gen_range(16000..32000);
    if p % 2 == 0 { p } else { p + 1 }
}

fn short(call_id: &str) -> String {
    call_id.chars().take(10).collect()
}
