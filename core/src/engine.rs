/// SIP 壓測主引擎
use crate::config::{Config, Transport};
use crate::rtp::{
    session::{RtpSession, RtpSessionConfig},
    stats::RtpStatsSnapshot,
};
use crate::sip::{Dialog, DialogState, SharedUdpSocket, SipMessage, SipParser};
use crate::sip_logger::{Direction, SipLogger, SipRole};
use crate::stats::{DetailedStats, FinalReport, LiveStats, StatsSnapshot};
use anyhow::Result;
use rand::Rng;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tracing::{debug, warn};

pub type ProgressCallback = Arc<dyn Fn(StatsSnapshot, f64) + Send + Sync>;

pub struct Engine {
    config: Config,
}

/// 內部事件：從接收 task 發給對話管理 task
#[derive(Debug)]
enum SipEvent {
    Response {
        call_id:         String,
        code:            u16,
        to_tag:          Option<String>,
        method:          Option<String>,
        /// 從 200 OK SDP 解析出的對端 RTP 地址（"ip:port"；其餘回應為 None）
        remote_rtp_addr: Option<String>,
        /// 從 200 OK Contact 標頭解析的 URI（ACK/BYE Request-URI 使用）
        contact_uri:     Option<String>,
    },
    /// 收到伺服器主動發來的 SIP 請求（RE-INVITE 刷新 session / 伺服器 BYE 掛斷）
    IncomingRequest {
        call_id: String,
        method:  String,
        /// 原始訊息（用於建構 200 OK 回應，鏡射 Via/From/To/CSeq）
        raw:     String,
    },
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn run(
        &self,
        on_progress: Option<ProgressCallback>,
    ) -> Result<FinalReport> {
        let cfg    = &self.config;
        let live   = Arc::new(LiveStats::default());
        let detail = Arc::new(DetailedStats::default());
        let start  = Instant::now();

        // ── TCP transport 目前尚未實作 ──
        if cfg.transport == Transport::Tcp {
            return Err(anyhow::anyhow!(
                "TCP transport 尚未實作，請使用 UDP（--transport udp）"
            ));
        }

        // ── 建立 SIP log 記錄器 ──
        let sip_log = Arc::new(
            SipLogger::new(&cfg.logs_dir, SipRole::Agent).unwrap_or_else(|e| {
                warn!("無法建立 SIP log：{}，改寫至系統暫存目錄", e);
                let tmp = std::env::temp_dir();
                let tmp_str = tmp.to_string_lossy();
                SipLogger::new(tmp_str.as_ref(), SipRole::Agent)
                    .expect("無法在系統暫存目錄建立 SIP log")
            }),
        );
        tracing::info!("SIP log → {}", sip_log.path.display());

        // 解析 server 地址
        let server_addr: SocketAddr = cfg.server_addr.parse()?;

        // 本機 IP：透過 connect() 探測真實出口 IP，禁止用 0.0.0.0
        let local_ip_owned: String = match cfg.local_addr.as_deref() {
            Some(ip) => ip.to_string(),
            None => {
                let probe = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
                probe.connect(&cfg.server_addr).await?;
                probe.local_addr()?.ip().to_string()
            }
        };
        let local_ip = local_ip_owned.as_str();

        // 建立共用 UDP socket
        let udp        = Arc::new(SharedUdpSocket::new(server_addr, local_ip).await?);
        let local_addr = udp.local_addr.clone();

        let local_domain_owned = cfg.local_domain.clone().unwrap_or_else(|| {
            local_addr.split(':').next().unwrap_or("127.0.0.1").to_string()
        });
        let local_domain = local_domain_owned.as_str();

        // 對話表（Call-ID → Dialog）
        let dialogs: Arc<Mutex<HashMap<String, Dialog>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // RTP session 表（Call-ID → RtpSession）
        let rtp_sessions: Arc<Mutex<HashMap<String, RtpSession>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // RTP port 分配計數器
        let rtp_port_counter = Arc::new(Mutex::new(
            if cfg.rtp_base_port % 2 == 0 { cfg.rtp_base_port }
            else { cfg.rtp_base_port + 1 },
        ));

        // 預分配但尚未啟動的 RTP socket（Call-ID → 已綁定 UdpSocket）
        // 解決 TOCTOU：INVITE 前綁定，200 OK 後移交 RtpSession
        let pre_bound_rtp: Arc<Mutex<HashMap<String, TokioUdpSocket>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // 累計 RTP 統計
        let rtp_snapshots: Arc<Mutex<Vec<RtpStatsSnapshot>>> =
            Arc::new(Mutex::new(Vec::new()));

        // 事件 channel
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<SipEvent>();

        // ── Task 1：接收 SIP 回應 ──
        let udp_recv  = Arc::clone(&udp);
        let ev_tx2    = ev_tx.clone();
        let log_recv  = Arc::clone(&sip_log);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                let n = match udp_recv.socket.recv(&mut buf).await {
                    Ok(n)  => n,
                    Err(_) => break,
                };
                let raw = String::from_utf8_lossy(&buf[..n]).into_owned();
                debug!("< {}", raw.lines().next().unwrap_or(""));

                log_recv.log_message(Direction::Recv, &raw, &udp_recv.server.to_string());

                // SipParser 支援 compact form（"i:" = Call-ID）
                let call_id = SipParser::call_id(&raw);

                if raw.starts_with("SIP/2.0") {
                    // ── SIP 回應 ──────────────────────────────────────────
                    let code            = SipParser::status_code(&raw);
                    let to_tag          = SipParser::to_tag(&raw);
                    let method          = SipParser::cseq_method(&raw);
                    // 取完整 "ip:port"（從 SDP c= + m= 行），fallback 用 SIP server IP
                    let remote_rtp_addr = if code == Some(200) && method.as_deref() == Some("INVITE") {
                        let sip_ip = udp_recv.server.ip().to_string();
                        SipResponse::sdp_rtp_addr(&raw, &sip_ip)
                    } else {
                        None
                    };
                    let contact_uri = if code == Some(200) { SipParser::contact_uri(&raw) } else { None };

                    if let (Some(call_id), Some(code)) = (call_id, code) {
                        let _ = ev_tx2.send(SipEvent::Response {
                            call_id, code, to_tag, method, remote_rtp_addr, contact_uri,
                        });
                    }
                } else {
                    // ── SIP 請求（伺服器主動送來：RE-INVITE / BYE 等）──────
                    let method_str = raw.lines().next()
                        .and_then(|l| l.split_whitespace().next())
                        .map(|s| s.to_uppercase())
                        .unwrap_or_default();

                    if method_str == "INVITE" || method_str == "BYE" {
                        if let Some(call_id) = call_id {
                            let _ = ev_tx2.send(SipEvent::IncomingRequest {
                                call_id, method: method_str, raw,
                            });
                        }
                    }
                }
            }
        });

        // ── Task 2：進度回報 ──
        if let Some(ref cb) = on_progress {
            let cb        = Arc::clone(cb);
            let live      = Arc::clone(&live);
            let duration  = cfg.duration_secs as f64;
            let unlimited = cfg.duration_secs == 0;
            tokio::spawn(async move {
                let mut interval = time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    let elapsed  = start.elapsed().as_secs_f64();
                    let progress = if unlimited { 0.0 } else { (elapsed / duration).min(1.0) };
                    cb(live.snapshot(), progress);
                    if !unlimited && elapsed >= duration { break; }
                }
            });
        }

        // ── 主控迴圈 ──
        let cps_interval   = Duration::from_secs_f64(1.0 / cfg.cps);
        let unlimited_time = cfg.duration_secs == 0;
        let deadline       = if unlimited_time {
            start + Duration::from_secs(u64::MAX / 2)
        } else {
            start + cfg.duration()
        };
        let invite_to = cfg.invite_timeout();
        let call_dur  = Duration::from_secs(cfg.call_duration_secs);

        let mut next_call = Instant::now();

        loop {
            let now = Instant::now();

            // ① 處理所有收到的 SIP 事件（回應 / 伺服器請求）
            while let Ok(ev) = ev_rx.try_recv() {
                match ev {
                SipEvent::Response { call_id, code, to_tag, method, remote_rtp_addr, contact_uri } => {
                // 先取出預分配的 RTP socket（若有），再鎖 dialogs，消除 TOCTOU
                let pre_socket = if cfg.enable_rtp && code == 200 && method.as_deref() == Some("INVITE") {
                    pre_bound_rtp.lock().await.remove(&call_id)
                } else {
                    None
                };

                let mut dialogs = dialogs.lock().await;

                if let Some(dialog) = dialogs.get_mut(&call_id) {
                    match code {
                        100 => dialog.on_trying(),
                        180 | 183 => {
                            dialog.on_ringing();
                            if let Some(pdd) = dialog.pdd_ms() {
                                detail.record_pdd(pdd);
                            }
                        }
                        200 if method.as_deref() == Some("INVITE") => {
                            let tag = to_tag.unwrap_or_default();
                            dialog.on_ok(tag.clone());
                            // 儲存對端 Contact URI（BYE/ACK Request-URI 使用）
                            dialog.remote_contact = contact_uri.clone();
                            live.on_answered();

                            if let Some(ms) = dialog.setup_time_ms() {
                                detail.record_setup(ms);
                            }

                            // 啟動 RTP session
                            if cfg.enable_rtp {
                                let pure_ip    = local_ip.split(':').next().unwrap_or(local_ip).to_string();
                                let server_ip  = cfg.server_addr.split(':').next().unwrap_or("127.0.0.1");
                                // SDP c= + m= 解析出完整 "ip:port"；fallback 為 SIP server IP:16384
                                let remote_addr = remote_rtp_addr
                                    .clone()
                                    .unwrap_or_else(|| format!("{}:16384", server_ip));
                                let local_pre  = dialog.local_rtp_port;
                                let audio_path = cfg.audio_file.clone();

                                let rtp_info = format!(
                                    "RTP START local={}:{} remote={} audio={:?} sdp_parsed={}",
                                    pure_ip,
                                    if local_pre > 0 { local_pre } else { cfg.rtp_base_port },
                                    remote_addr,
                                    audio_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "silence".into()),
                                    remote_rtp_addr.is_some(),
                                );
                                sip_log.log_event(&call_id, &rtp_info);
                                tracing::info!("{}", rtp_info);

                                let rtp_cfg = RtpSessionConfig {
                                    base_port:   cfg.rtp_base_port,
                                    local_ip:    pure_ip,
                                    remote_addr,
                                    audio_file:  audio_path,
                                    ssrc:        None,
                                    local_port:  if local_pre > 0 { Some(local_pre) } else { None },
                                };
                                let pc                 = Arc::clone(&rtp_port_counter);
                                let rtp_sessions_clone = Arc::clone(&rtp_sessions);
                                let log_rtp            = Arc::clone(&sip_log);
                                let live_rtp           = Arc::clone(&live);
                                let cid                = call_id.clone();
                                tokio::spawn(async move {
                                    match RtpSession::start(rtp_cfg, pc, pre_socket).await {
                                        Ok(session) => {
                                            live_rtp.on_rtp_start();
                                            rtp_sessions_clone.lock().await.insert(cid, session);
                                        }
                                        Err(e) => {
                                            let msg = format!("RTP session 啟動失敗: {}", e);
                                            warn!("{}", msg);
                                            log_rtp.log_event(&cid, &msg);
                                        }
                                    }
                                });
                            }

                            // 送 ACK（2xx ACK 使用 Contact URI 作為 Request-URI）
                            let ack = SipMessage::ack(
                                &dialog.call_id,
                                &cfg.caller_number,
                                local_domain,
                                &dialog.callee,
                                &tag,
                                &cfg.server_addr,
                                &local_addr,
                                dialog.cseq,
                                &SipMessage::new_branch(),
                                &dialog.from_tag,
                                "UDP",
                                dialog.remote_contact.as_deref(),
                            );
                            let udp_ack  = Arc::clone(&udp);
                            let log_ack  = Arc::clone(&sip_log);
                            let server   = cfg.server_addr.clone();
                            let ack_copy = ack.clone();
                            tokio::spawn(async move {
                                log_ack.log_message(Direction::Send, &ack_copy, &server);
                                let _ = udp_ack.send(&ack_copy).await;
                            });
                        }
                        200 if method.as_deref() == Some("BYE") => {
                            if let Some(dur) = dialog.call_duration_secs() {
                                detail.record_duration(dur);
                            }
                            if cfg.enable_rtp {
                                let cid   = call_id.clone();
                                let rtp_s = Arc::clone(&rtp_sessions);
                                let snaps = Arc::clone(&rtp_snapshots);
                                let live_rtp = Arc::clone(&live);
                                tokio::spawn(async move {
                                    let mut sessions = rtp_s.lock().await;
                                    if let Some(session) = sessions.remove(&cid) {
                                        let snap = session.stop();
                                        live_rtp.on_rtp_stop();
                                        snaps.lock().await.push(snap);
                                    }
                                });
                            }
                            dialog.on_bye_ok();
                            live.on_completed();
                        }
                        400..=699 if method.as_deref() != Some("BYE") => {
                            // non-2xx ACK：使用原始 INVITE branch（RFC 3261 §17.1.1.3）
                            let to_tag_for_ack = to_tag.as_deref().unwrap_or("").to_string();
                            let ack_err = SipMessage::ack(
                                &dialog.call_id,
                                &cfg.caller_number,
                                local_domain,
                                &dialog.callee,
                                &to_tag_for_ack,
                                &cfg.server_addr,
                                &local_addr,
                                dialog.cseq,
                                &dialog.branch,
                                &dialog.from_tag,
                                "UDP",
                                None,  // non-2xx 不使用 Contact URI
                            );
                            let udp_ack  = Arc::clone(&udp);
                            let log_ack  = Arc::clone(&sip_log);
                            let srv_ack  = cfg.server_addr.clone();
                            let ack_copy = ack_err.clone();
                            tokio::spawn(async move {
                                log_ack.log_message(Direction::Send, &ack_copy, &srv_ack);
                                let _ = udp_ack.send(&ack_copy).await;
                            });

                            // 同時清理任何預分配但未使用的 RTP socket
                            if cfg.enable_rtp {
                                pre_bound_rtp.lock().await.remove(&call_id);
                            }

                            detail.record_fail_code(code);
                            dialog.on_error(code);
                            live.on_failed();
                        }
                        _ => {}
                    }
                }
                } // SipEvent::Response
                SipEvent::IncomingRequest { call_id, method, raw: req_raw } => {
                    let mut dialogs = dialogs.lock().await;
                    if let Some(dialog) = dialogs.get_mut(&call_id) {
                        match method.as_str() {
                            "INVITE" => {
                                // RE-INVITE（Session-Expires 刷新）→ 200 OK 保持通話
                                sip_log.log_event(&call_id, "收到 RE-INVITE（Session-Expires），回應 200 OK");
                                let local_port = if dialog.local_rtp_port > 0 {
                                    dialog.local_rtp_port
                                } else {
                                    cfg.rtp_base_port
                                };
                                let ok      = SipMessage::ok_for_server_reinvite(&req_raw, &local_addr, local_port);
                                let udp_ok  = Arc::clone(&udp);
                                let log_ok  = Arc::clone(&sip_log);
                                let server  = cfg.server_addr.clone();
                                let ok_copy = ok.clone();
                                tokio::spawn(async move {
                                    log_ok.log_message(Direction::Send, &ok_copy, &server);
                                    let _ = udp_ok.send(&ok_copy).await;
                                });
                            }
                            "BYE" => {
                                // 伺服器主動掛斷 → 200 OK 並結束本通話
                                sip_log.log_event(&call_id, "收到伺服器 BYE，回應 200 OK");
                                let ok      = SipMessage::ok_for_server_bye(&req_raw);
                                let udp_ok  = Arc::clone(&udp);
                                let log_ok  = Arc::clone(&sip_log);
                                let server  = cfg.server_addr.clone();
                                let ok_copy = ok.clone();
                                tokio::spawn(async move {
                                    log_ok.log_message(Direction::Send, &ok_copy, &server);
                                    let _ = udp_ok.send(&ok_copy).await;
                                });

                                if cfg.enable_rtp && matches!(dialog.state, DialogState::Connected) {
                                    if let Some(a) = dialog.answered_at {
                                        detail.record_duration(a.elapsed().as_secs_f64());
                                    }
                                    let cid      = call_id.clone();
                                    let rtp_s    = Arc::clone(&rtp_sessions);
                                    let snaps    = Arc::clone(&rtp_snapshots);
                                    let live_rtp = Arc::clone(&live);
                                    tokio::spawn(async move {
                                        let mut sessions = rtp_s.lock().await;
                                        if let Some(session) = sessions.remove(&cid) {
                                            let snap = session.stop();
                                            live_rtp.on_rtp_stop();
                                            snaps.lock().await.push(snap);
                                        }
                                    });
                                }

                                if matches!(dialog.state, DialogState::Connected | DialogState::Terminating) {
                                    dialog.on_bye_ok();
                                    live.on_completed();
                                }
                            }
                            _ => {}
                        }
                    }
                } // SipEvent::IncomingRequest
                } // match ev
            }

            // ② 掃描逾時 & 應該 BYE 的通話
            {
                let mut dialogs = dialogs.lock().await;
                let mut to_send: Vec<String> = Vec::new();

                for dialog in dialogs.values_mut() {
                    match &dialog.state {
                        // INVITE 逾時 → 發 CANCEL
                        DialogState::Calling | DialogState::Trying | DialogState::Ringing => {
                            if now.duration_since(dialog.invite_sent_at) > invite_to {
                                sip_log.log_event(&dialog.call_id, "TIMEOUT — 發送 CANCEL");
                                let cancel = SipMessage::cancel(
                                    &dialog.call_id,
                                    &cfg.caller_number,
                                    local_domain,
                                    &dialog.callee,
                                    &cfg.server_addr,
                                    &local_addr,
                                    dialog.cseq,
                                    &dialog.branch,
                                    &dialog.from_tag,
                                    "UDP",
                                );
                                to_send.push(cancel);
                                dialog.on_timeout();
                                live.on_timeout();
                            }
                        }
                        // 通話時長到 → 發 BYE（使用 Contact URI 作為 Request-URI）
                        DialogState::Connected => {
                            if let Some(ans) = dialog.answered_at {
                                if now.duration_since(ans) >= call_dur && call_dur.as_secs() > 0 {
                                    let bye_branch = SipMessage::new_branch();
                                    let bye = SipMessage::bye(
                                        &dialog.call_id,
                                        &cfg.caller_number,
                                        local_domain,
                                        &dialog.callee,
                                        dialog.to_tag.as_deref().unwrap_or(""),
                                        &cfg.server_addr,
                                        &local_addr,
                                        dialog.cseq + 1,
                                        &bye_branch,
                                        &dialog.from_tag,
                                        "UDP",
                                        dialog.remote_contact.as_deref(),
                                    );
                                    dialog.on_bye_sent();
                                    to_send.push(bye);
                                }
                            }
                        }
                        // BYE 逾時（對端未回 200 OK）→ 強制結束通話
                        DialogState::Terminating => {
                            if let Some(bye_sent) = dialog.bye_sent_at {
                                if now.duration_since(bye_sent) > invite_to {
                                    sip_log.log_event(&dialog.call_id, "BYE TIMEOUT — 強制結束通話");
                                    if cfg.enable_rtp {
                                        let cid   = dialog.call_id.clone();
                                        let rtp_s = Arc::clone(&rtp_sessions);
                                        let snaps = Arc::clone(&rtp_snapshots);
                                        tokio::spawn(async move {
                                            let mut sessions = rtp_s.lock().await;
                                            if let Some(session) = sessions.remove(&cid) {
                                                snaps.lock().await.push(session.stop());
                                            }
                                        });
                                    }
                                    if let Some(dur) = dialog.call_duration_secs() {
                                        detail.record_duration(dur);
                                    }
                                    dialog.on_bye_ok(); // 強制標記為 Completed
                                    live.on_completed();
                                }
                            }
                        }
                        _ => {}
                    }
                }

                for msg in to_send {
                    let udp_s   = Arc::clone(&udp);
                    let log_bye = Arc::clone(&sip_log);
                    let server  = cfg.server_addr.clone();
                    let msg_log = msg.clone();
                    tokio::spawn(async move {
                        log_bye.log_message(Direction::Send, &msg_log, &server);
                        let _ = udp_s.send(&msg_log).await;
                    });
                }

                // 清理已結束的 dialog
                dialogs.retain(|_, d| matches!(
                    d.state,
                    DialogState::Calling  | DialogState::Trying |
                    DialogState::Ringing  | DialogState::Connected |
                    DialogState::Terminating
                ));
            }

            // ③ 發起新通話
            let total_limit_reached = cfg.max_total_calls
                .map_or(false, |max| live.calls_initiated.load(std::sync::atomic::Ordering::Relaxed) >= max);

            if !total_limit_reached && now >= next_call && now < deadline {
                let concurrent = {
                    let dialogs = dialogs.lock().await;
                    dialogs.values().filter(|d| matches!(
                        d.state,
                        DialogState::Calling | DialogState::Trying |
                        DialogState::Ringing | DialogState::Connected
                    )).count()
                };

                if concurrent < cfg.max_concurrent_calls {
                    let callee = match &cfg.callee_fixed {
                        Some(num) if !num.is_empty() => num.clone(),
                        _ => format!(
                            "{}{}",
                            cfg.callee_prefix,
                            rand::thread_rng().gen_range(0..=cfg.callee_range)
                        ),
                    };

                    let call_id  = SipMessage::new_call_id(local_domain);
                    let from_tag = SipMessage::new_tag();
                    let branch   = SipMessage::new_branch();

                    // 預分配 RTP port（保持 socket 綁定以避免 TOCTOU）
                    let (rtp_port, pre_socket) = if cfg.enable_rtp {
                        let pc = Arc::clone(&rtp_port_counter);
                        let ip = local_ip.split(':').next().unwrap_or(local_ip).to_string();
                        match RtpSession::allocate_port(&pc, &ip).await {
                            Ok((p, sock)) => (p, Some(sock)),
                            Err(e) => {
                                warn!("RTP port 預分配失敗: {}", e);
                                (cfg.rtp_base_port, None)
                            }
                        }
                    } else {
                        (0, None)
                    };

                    // 儲存預分配的 socket
                    if let Some(sock) = pre_socket {
                        pre_bound_rtp.lock().await.insert(call_id.clone(), sock);
                    }

                    let invite = SipMessage::invite(
                        &call_id,
                        &cfg.caller_number,
                        local_domain,
                        &callee,
                        &cfg.server_addr,
                        &local_addr,
                        1,
                        &branch,
                        &from_tag,
                        "UDP",
                        if rtp_port > 0 { rtp_port } else { 9 },
                    );

                    let dialog = Dialog::new(call_id.clone(), from_tag, branch, callee, rtp_port);
                    dialogs.lock().await.insert(call_id, dialog);
                    live.on_invite();

                    let udp_inv    = Arc::clone(&udp);
                    let log_invite = Arc::clone(&sip_log);
                    let server     = cfg.server_addr.clone();
                    let inv_log    = invite.clone();
                    tokio::spawn(async move {
                        log_invite.log_message(Direction::Send, &inv_log, &server);
                        let _ = udp_inv.send(&inv_log).await;
                    });

                    next_call = now + cps_interval;
                }
            }

            // ④ 測試結束條件
            let all_done = total_limit_reached
                && live.calls_active.load(std::sync::atomic::Ordering::Relaxed) <= 0;

            if now >= deadline || all_done {
                // 等待剩餘通話結束（最多 invite_timeout 秒，替換原本硬寫的 2 秒）
                let drain_deadline = Instant::now() + invite_to;
                loop {
                    let active = live.calls_active.load(std::sync::atomic::Ordering::Relaxed);
                    if active <= 0 || Instant::now() >= drain_deadline { break; }
                    time::sleep(Duration::from_millis(200)).await;
                }
                break;
            }

            time::sleep(Duration::from_micros(500)).await;
        }

        // ── 產生最終報告 ──
        let snap    = live.snapshot();
        let elapsed = start.elapsed().as_secs_f64();

        sip_log.log_summary(&format!(
            "發起={} 接通={} 完成={} 失敗={} 逾時={} ASR={:.1}% 時長={:.1}s",
            snap.calls_initiated, snap.calls_answered, snap.calls_completed,
            snap.calls_failed, snap.calls_timeout, snap.asr, elapsed,
        ));

        // 聚合 RTP 統計
        let rtp_agg: Option<(f64, f64, f64, u64, u64, u64)> = if cfg.enable_rtp {
            {
                let mut sessions = rtp_sessions.lock().await;
                let mut snaps    = rtp_snapshots.lock().await;
                for (_, s) in sessions.drain() { snaps.push(s.stop()); }
            }
            let snaps = rtp_snapshots.lock().await;
            if snaps.is_empty() {
                None
            } else {
                let n          = snaps.len() as f64;
                let avg_mos    = snaps.iter().map(|s| s.mos).sum::<f64>()           / n;
                let avg_loss   = snaps.iter().map(|s| s.loss_rate_pct).sum::<f64>() / n;
                let avg_jitter = snaps.iter().map(|s| s.jitter_ms).sum::<f64>()     / n;
                let total_sent = snaps.iter().map(|s| s.sent_packets).sum::<u64>();
                let total_recv = snaps.iter().map(|s| s.recv_packets).sum::<u64>();
                let total_ooo  = snaps.iter().map(|s| s.out_of_order).sum::<u64>();
                Some((avg_mos, avg_loss, avg_jitter, total_sent, total_recv, total_ooo))
            }
        } else {
            None
        };

        // HDR histogram 數據（std::sync::MutexGuard，不可跨 await）
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

        let fail_codes = detail.fail_codes.lock().unwrap().clone();

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
            actual_cps:   snap.calls_initiated as f64 / elapsed,
            pdd_p50_ms:   pdd_p50,
            pdd_p95_ms:   pdd_p95,
            pdd_p99_ms:   pdd_p99,
            pdd_max_ms:   pdd_max,
            setup_p50_ms: setup_p50,
            setup_p95_ms: setup_p95,
            setup_p99_ms: setup_p99,
            setup_max_ms: setup_max,
            acd_secs,
            fail_4xx: detail.fail_4xx.load(std::sync::atomic::Ordering::Relaxed),
            fail_5xx: detail.fail_5xx.load(std::sync::atomic::Ordering::Relaxed),
            fail_6xx: detail.fail_6xx.load(std::sync::atomic::Ordering::Relaxed),
            fail_codes,
            mos:              rtp_agg.map(|a| a.0),
            loss_rate_pct:    rtp_agg.map(|a| a.1),
            jitter_ms:        rtp_agg.map(|a| a.2),
            rtp_sent:         rtp_agg.map(|a| a.3),
            rtp_recv:         rtp_agg.map(|a| a.4),
            rtp_out_of_order: rtp_agg.map(|a| a.5),
        })
    }
}
