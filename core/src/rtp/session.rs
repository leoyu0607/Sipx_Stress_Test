/// 單通通話的 RTP Session
/// 負責：
///   1. 綁定 UDP port（依設定的起始 port 動態分配）
///   2. 每 20ms 傳送一個 G.711 frame
///   3. 接收對端 RTP，計算 jitter/loss
///   4. 通話結束時產生 RtpStatsSnapshot

use crate::rtp::{
    audio::AudioSource,
    packet::{RtpPacket, PT_PCMU},
    stats::{RtpStats, RtpStatsSnapshot},
};
use anyhow::{Context, Result};
use rand::Rng;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::UdpSocket, sync::Mutex, time};
use tracing::debug;

/// RTP session 設定
#[derive(Debug, Clone)]
pub struct RtpSessionConfig {
    /// RTP 起始 port（系統從此 port 往上找可用的 even port）
    pub base_port:   u16,
    /// 本機 IP
    pub local_ip:    String,
    /// 遠端 RTP 地址（從 SDP 解析，或用 SIP server IP + 預設 port）
    pub remote_addr: String,
    /// 音檔路徑（None = 靜音）
    pub audio_file:  Option<std::path::PathBuf>,
    /// SSRC（None = 隨機）
    pub ssrc:        Option<u32>,
}

/// 單通通話的 RTP session handle
pub struct RtpSession {
    pub stats:      Arc<RtpStats>,
    stop_flag:      Arc<AtomicBool>,
    local_rtp_port: u16,
}

impl RtpSession {
    /// 啟動 RTP session（非同步，spawn 兩個 task：傳送 + 接收）
    pub async fn start(
        config: RtpSessionConfig,
        port_counter: Arc<Mutex<u16>>,
    ) -> Result<Self> {
        // ── 動態分配 RTP port ──
        let local_port = Self::allocate_port(&port_counter, &config.local_ip).await?;

        let bind_addr  = format!("{}:{}", config.local_ip, local_port);
        let socket     = UdpSocket::bind(&bind_addr)
            .await
            .with_context(|| format!("無法綁定 RTP socket: {}", bind_addr))?;

        socket.connect(&config.remote_addr)
            .await
            .with_context(|| format!("無法連接 RTP 對端: {}", config.remote_addr))?;

        let socket     = Arc::new(socket);
        let stats      = Arc::new(RtpStats::new());
        let stop_flag  = Arc::new(AtomicBool::new(false));
        let ssrc       = config.ssrc.unwrap_or_else(|| rand::thread_rng().gen());

        // ── Task A：傳送（每 20ms 一個 frame）──
        {
            let socket    = Arc::clone(&socket);
            let stats     = Arc::clone(&stats);
            let stop      = Arc::clone(&stop_flag);
            let audio_path = config.audio_file.clone();

            tokio::spawn(async move {
                let mut source = match &audio_path {
                    Some(p) => AudioSource::from_file(p)
                        .unwrap_or_else(|e| {
                            tracing::warn!("音檔載入失敗（{:?}），改用靜音: {}", p, e);
                            AudioSource::silence()
                        }),
                    None => AudioSource::silence(),
                };

                let mut seq: u16 = rand::thread_rng().gen();
                let mut ts:  u32 = rand::thread_rng().gen();
                let ptime = Duration::from_millis(20);
                let mut interval = time::interval(ptime);

                loop {
                    interval.tick().await;
                    if stop.load(Ordering::Relaxed) { break; }

                    let frame = match source.next_frame() {
                        Some(f) => f,
                        None    => break,  // 非循環模式播完
                    };

                    let pkt   = RtpPacket::new(PT_PCMU, seq, ts, ssrc, frame.clone());
                    let bytes = pkt.encode();

                    stats.on_send(frame.len());

                    if let Err(e) = socket.send(&bytes).await {
                        debug!("RTP 傳送失敗: {}", e);
                        break;
                    }

                    seq = seq.wrapping_add(1);
                    ts  = ts.wrapping_add(160); // 20ms @ 8kHz
                }
            });
        }

        // ── Task B：接收（計算 jitter/loss）──
        {
            let socket = Arc::clone(&socket);
            let stats  = Arc::clone(&stats);
            let stop   = Arc::clone(&stop_flag);

            tokio::spawn(async move {
                let mut buf = vec![0u8; 1500];
                loop {
                    if stop.load(Ordering::Relaxed) { break; }

                    // 設定 50ms 逾時，讓 stop_flag 有機會被偵測
                    match time::timeout(
                        Duration::from_millis(50),
                        socket.recv(&mut buf),
                    ).await {
                        Ok(Ok(n)) => {
                            if let Some(pkt) = RtpPacket::decode(&buf[..n]) {
                                let now_us = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_micros() as u64;
                                stats.on_recv(pkt.sequence, pkt.timestamp, now_us);
                                debug!("RTP recv seq={} ts={}", pkt.sequence, pkt.timestamp);
                            }
                        }
                        Ok(Err(e)) => {
                            debug!("RTP 接收錯誤: {}", e);
                            break;
                        }
                        Err(_) => {} // timeout，繼續迴圈
                    }
                }
            });
        }

        Ok(Self { stats, stop_flag, local_rtp_port: local_port })
    }

    /// 停止 RTP session，回傳統計快照
    pub fn stop(&self) -> RtpStatsSnapshot {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.stats.snapshot()
    }

    /// 本機 RTP port（供 SDP 填寫）
    pub fn local_port(&self) -> u16 {
        self.local_rtp_port
    }

    // ── Port 分配 ────────────────────────────────────────────────

    /// 從 base_port 開始，找到可用的 even port
    async fn allocate_port(
        counter:  &Arc<Mutex<u16>>,
        local_ip: &str,
    ) -> Result<u16> {
        let mut guard = counter.lock().await;
        let start = *guard;
        // 嘗試最多 500 個 port
        for offset in (0u16..1000).step_by(2) {
            let port = start.wrapping_add(offset);
            if port < 1024 { continue; }
            let addr = format!("{}:{}", local_ip, port);
            if UdpSocket::bind(&addr).await.is_ok() {
                // 下次從這個 port + 2 開始
                *guard = port.wrapping_add(2);
                return Ok(port);
            }
        }
        anyhow::bail!("無法找到可用的 RTP port（從 {} 開始）", start);
    }
}
