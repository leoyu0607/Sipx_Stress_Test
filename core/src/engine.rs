/// SIP 壓測主引擎
use crate::config::{Config, Transport};
use crate::rtp::{
    session::{RtpSession, RtpSessionConfig},
    stats::RtpStatsSnapshot,
};
use crate::sip::{Dialog, DialogState, SharedUdpSocket, SipMessage, SipResponse};
use crate::sip_logger::{Direction, SipLogger, SipRole};
use crate::stats::{DetailedStats, FinalReport, LiveStats, StatsSnapshot};
use anyhow::Result;
use rand::Rng;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tracing::debug;

pub type ProgressCallback = Arc<dyn Fn(StatsSnapshot, f64) + Send + Sync>;

pub struct Engine {
    config: Config,
}

/// 內部事件：從接收 task 發給對話管理 task
#[derive(Debug)]
enum SipEvent {
    Response {
        call_id: String,
        code:    u16,
        to_tag:  Option<String>,
        method:  Option<String>,
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
        let cfg     = &self.config;
        let live    = Arc::new(LiveStats::default());
        let detail  = Arc::new(DetailedStats::default());
        let start   = Instant::now();

        // ── 建立 SIP log 記錄器（agent = 壓測發起方）──
        let sip_log = Arc::new(
            SipLogger::new(&cfg.logs_dir, SipRole::Agent)
                .unwrap_or_else(|e| {
                    eprintln!("[sipress] 警告：無法建立 SIP log：{}", e);
                    SipLogger::new("/tmp", SipRole::Agent).unwrap()
                })
        );
        eprintln!("[sipress] SIP log → {}", sip_log.path.display());

        // 解析 server 地址
        let server_addr: SocketAddr = cfg.server_addr.parse()?;

        // 本機 IP
        let local_ip = cfg.local_addr.as_deref().unwrap_or("0.0.0.0");
        let local_domain = cfg.local_domain.as_deref()
            .unwrap_or_else(|| local_ip.split(':').next().unwrap_or(local_ip));

        // 建立共用 UDP socket
        let udp = Arc::new(SharedUdpSocket::new(server_addr, local_ip).await?);
        let local_addr = udp.local_addr.clone();

        // 對話表（Call-ID → Dialog）
        let dialogs: Arc<Mutex<HashMap<String, Dialog>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // RTP session 表（Call-ID → RtpSession）
        let rtp_sessions: Arc<Mutex<HashMap<String, RtpSession>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // RTP port 分配計數器（確保每通 call 用不同 port）
        let rtp_port_counter = Arc::new(Mutex::new(
            if cfg.rtp_base_port % 2 == 0 { cfg.rtp_base_port }
            else { cfg.rtp_base_port + 1 }  // 強制偶數
        ));

        // 累計 RTP 統計（聚合所有通話）
        let rtp_snapshots: Arc<Mutex<Vec<RtpStatsSnapshot>>> =
            Arc::new(Mutex::new(Vec::new()));

        // 事件 channel：接收 task → 主控 task
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<SipEvent>();

        // ── Task 1：接收 SIP 回應（唯一接收迴圈）──
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

                // 記錄原始 SIP 回應
                log_recv.log_message(Direction::Recv, &raw, &udp_recv.server.to_string());

                let code   = SipResponse::status_code(&raw);
                let to_tag = SipResponse::to_tag(&raw);
                let method = SipResponse::cseq_method(&raw);

                let call_id = raw.lines()
                    .find(|l| l.to_lowercase().starts_with("call-id:"))
                    .and_then(|l| l.splitn(2, ':').nth(1))
                    .map(|s| s.trim().to_string());

                if let (Some(call_id), Some(code)) = (call_id, code) {
                    let _ = ev_tx2.send(SipEvent::Response {
                        call_id, code, to_tag, method,
                    });
                }
            }
        });

        // ── Task 2：進度回報 ──
        if let Some(ref cb) = on_progress {
            let cb        = Arc::clone(cb);
            let live      = Arc::clone(&live);
            let duration  = cfg.duration_secs as f64;
            tokio::spawn(async move {
                let mut interval = time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    let elapsed = start.elapsed().as_secs_f64();
                    cb(live.snapshot(), (elapsed / duration).min(1.0));
                    if elapsed >= duration { break; }
                }
            });
        }

        // ── 主控迴圈 ──
        let cps_interval = Duration::from_secs_f64(1.0 / cfg.cps);
        let deadline     = start + cfg.duration();
        let invite_to    = cfg.invite_timeout();
        let call_dur     = Duration::from_secs(cfg.call_duration_secs);

        let mut next_call = Instant::now();

        loop {
            let now = Instant::now();

            // ① 處理所有收到的 SIP 回應
            while let Ok(ev) = ev_rx.try_recv() {
                let SipEvent::Response { call_id, code, to_tag, method } = ev;
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
                            live.on_answered();

                            if let Some(ms) = dialog.setup_time_ms() {
                                detail.record_setup(ms);
                            }

                            // 啟動 RTP session（若已啟用）
                            if cfg.enable_rtp {
                                let pure_ip = local_ip.split(':').next().unwrap_or("0.0.0.0").to_string();
                                let rtp_cfg = RtpSessionConfig {
                                    base_port:   cfg.rtp_base_port,
                                    local_ip:    pure_ip,
                                    // 對端 RTP：使用 server IP + port 預設 (16384)
                                    // 實際應從 SDP 解析，此處簡化
                                    remote_addr: format!(
                                        "{}:16384",
                                        cfg.server_addr.split(':').next().unwrap_or("127.0.0.1")
                                    ),
                                    audio_file:  cfg.audio_file.clone(),
                                    ssrc:        None,
                                };
                                let pc = Arc::clone(&rtp_port_counter);
                                let rtp_sessions_clone = Arc::clone(&rtp_sessions);
                                let cid = call_id.clone();
                                tokio::spawn(async move {
                                    match RtpSession::start(rtp_cfg, pc).await {
                                        Ok(session) => {
                                            rtp_sessions_clone.lock().await.insert(cid, session);
                                        }
                                        Err(e) => {
                                            tracing::warn!("RTP session 啟動失敗: {}", e);
                                        }
                                    }
                                });
                            }

                            // 送 ACK
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
                            );
                            let udp     = Arc::clone(&udp);
                            let log_ack = Arc::clone(&sip_log);
                            let server  = cfg.server_addr.clone();
                            let ack_log = ack.clone();
                            tokio::spawn(async move {
                                log_ack.log_message(Direction::Send, &ack_log, &server);
                                let _ = udp.send(&ack_log).await;
                            });
                        }
                        200 if method.as_deref() == Some("BYE") => {
                            if let Some(dur) = dialog.call_duration_secs() {
                                detail.record_duration(dur);
                            }
                            // 停止 RTP session，收集統計
                            if cfg.enable_rtp {
                                let cid = call_id.clone();
                                let rtp_s = Arc::clone(&rtp_sessions);
                                let snaps = Arc::clone(&rtp_snapshots);
                                tokio::spawn(async move {
                                    let mut sessions = rtp_s.lock().await;
                                    if let Some(session) = sessions.remove(&cid) {
                                        let snap = session.stop();
                                        snaps.lock().await.push(snap);
                                    }
                                });
                            }
                            dialog.on_bye_ok();
                            live.on_completed();
                        }
                        400..=699 if method.as_deref() != Some("BYE") => {
                            detail.record_fail_code(code);
                            dialog.on_error(code);
                            live.on_failed();
                        }
                        _ => {}
                    }
                }
            }

            // ② 掃描逾時 & 應該 BYE 的通話
            {
                let mut dialogs = dialogs.lock().await;
                let mut to_send: Vec<String> = Vec::new();

                for dialog in dialogs.values_mut() {
                    match &dialog.state {
                        // 逾時檢查
                        DialogState::Calling | DialogState::Trying | DialogState::Ringing => {
                            if now.duration_since(dialog.invite_sent_at) > invite_to {
                                sip_log.log_event(&dialog.call_id, "TIMEOUT — 未收到 180/200");
                                dialog.on_timeout();
                                live.on_timeout();
                            }
                        }
                        // 接通後檢查是否要掛斷
                        DialogState::Connected => {
                            if let Some(ans) = dialog.answered_at {
                                if now.duration_since(ans) >= call_dur && call_dur.as_secs() > 0 {
                                    // 送 BYE
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
                                    );
                                    dialog.on_bye_sent();
                                    to_send.push(bye);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                for msg in to_send {
                    let udp     = Arc::clone(&udp);
                    let log_bye = Arc::clone(&sip_log);
                    let server  = cfg.server_addr.clone();
                    let msg_log = msg.clone();
                    tokio::spawn(async move {
                        log_bye.log_message(Direction::Send, &msg_log, &server);
                        let _ = udp.send(&msg_log).await;
                    });
                }

                // 清理已結束的 dialog
                dialogs.retain(|_, d| matches!(
                    d.state,
                    DialogState::Calling | DialogState::Trying |
                    DialogState::Ringing | DialogState::Connected |
                    DialogState::Terminating
                ));
            }

            // ③ 發起新通話（受 CPS 和並發上限控制）
            if now >= next_call && now < deadline {
                let concurrent = {
                    let dialogs = dialogs.lock().await;
                    dialogs.values().filter(|d| matches!(
                        d.state,
                        DialogState::Calling | DialogState::Trying |
                        DialogState::Ringing | DialogState::Connected
                    )).count()
                };

                if concurrent < cfg.max_concurrent_calls {
                    // 隨機被叫號碼
                    let callee = format!(
                        "{}{}",
                        cfg.callee_prefix,
                        rand::thread_rng().gen_range(0..=cfg.callee_range)
                    );

                    let call_id  = SipMessage::new_call_id(local_domain);
                    let from_tag = SipMessage::new_tag();
                    let branch   = SipMessage::new_branch();

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
                    );

                    let dialog = Dialog::new(call_id.clone(), from_tag, branch, callee);
                    dialogs.lock().await.insert(call_id, dialog);
                    live.on_invite();

                    let udp        = Arc::clone(&udp);
                    let log_invite = Arc::clone(&sip_log);
                    let server     = cfg.server_addr.clone();
                    let inv_log    = invite.clone();
                    tokio::spawn(async move {
                        log_invite.log_message(Direction::Send, &inv_log, &server);
                        let _ = udp.send(&inv_log).await;
                    });

                    next_call = now + cps_interval;
                }
            }

            // ④ 測試結束
            if now >= deadline {
                // 等待剩餘通話結束（最多再等 invite_timeout）
                time::sleep(Duration::from_secs(2)).await;
                break;
            }

            // 短暫睡眠避免 CPU 空轉（500µs = 足夠響應 2000 CPS）
            time::sleep(Duration::from_micros(500)).await;
        }

        // ── 產生最終報告 ──
        let snap      = live.snapshot();
        let elapsed   = start.elapsed().as_secs_f64();

        // 寫摘要到 SIP log 尾端
        sip_log.log_summary(&format!(
            "發起={} 接通={} 完成={} 失敗={} 逾時={} ASR={:.1}% 時長={:.1}s",
            snap.calls_initiated, snap.calls_answered, snap.calls_completed,
            snap.calls_failed, snap.calls_timeout, snap.asr, elapsed,
        ));
        let pdd_h     = detail.pdd_hist.lock().unwrap();
        let setup_h   = detail.setup_hist.lock().unwrap();
        let dur_h     = detail.dur_hist.lock().unwrap();

        let us_to_ms  = |h: &hdrhistogram::Histogram<u64>, q: f64| h.value_at_quantile(q) as f64 / 1000.0;
        let acd = if snap.calls_completed > 0 {
            dur_h.mean() / 1000.0  // ms → s
        } else { 0.0 };

        // ── 聚合 RTP 統計 ──
        let rtp_agg: Option<(f64, f64, f64, u64, u64, u64)> = if cfg.enable_rtp {
            // 停止仍在執行的 RTP sessions（通話未正常結束）
            {
                let mut sessions = rtp_sessions.lock().await;
                let mut snaps = rtp_snapshots.lock().await;
                for (_, s) in sessions.drain() {
                    snaps.push(s.stop());
                }
            }
            let snaps = rtp_snapshots.lock().await;
            if snaps.is_empty() {
                None
            } else {
                let n = snaps.len() as f64;
                let avg_mos     = snaps.iter().map(|s| s.mos).sum::<f64>()           / n;
                let avg_loss    = snaps.iter().map(|s| s.loss_rate_pct).sum::<f64>() / n;
                let avg_jitter  = snaps.iter().map(|s| s.jitter_ms).sum::<f64>()     / n;
                let total_sent  = snaps.iter().map(|s| s.sent_packets).sum::<u64>();
                let total_recv  = snaps.iter().map(|s| s.recv_packets).sum::<u64>();
                let total_ooo   = snaps.iter().map(|s| s.out_of_order).sum::<u64>();
                Some((avg_mos, avg_loss, avg_jitter, total_sent, total_recv, total_ooo))
            }
        } else {
            None
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
            actual_cps: snap.calls_initiated as f64 / elapsed,
            pdd_p50_ms:  us_to_ms(&pdd_h, 0.50),
            pdd_p95_ms:  us_to_ms(&pdd_h, 0.95),
            pdd_p99_ms:  us_to_ms(&pdd_h, 0.99),
            pdd_max_ms:  pdd_h.max() as f64 / 1000.0,
            setup_p50_ms: us_to_ms(&setup_h, 0.50),
            setup_p95_ms: us_to_ms(&setup_h, 0.95),
            setup_p99_ms: us_to_ms(&setup_h, 0.99),
            setup_max_ms: setup_h.max() as f64 / 1000.0,
            acd_secs:    acd,
            fail_4xx: detail.fail_4xx.load(std::sync::atomic::Ordering::Relaxed),
            fail_5xx: detail.fail_5xx.load(std::sync::atomic::Ordering::Relaxed),
            fail_6xx: detail.fail_6xx.load(std::sync::atomic::Ordering::Relaxed),
            mos:           rtp_agg.map(|a| a.0),
            loss_rate_pct: rtp_agg.map(|a| a.1),
            jitter_ms:     rtp_agg.map(|a| a.2),
            rtp_sent:      rtp_agg.map(|a| a.3),
            rtp_recv:      rtp_agg.map(|a| a.4),
            rtp_out_of_order: rtp_agg.map(|a| a.5),
        })
    }
}
