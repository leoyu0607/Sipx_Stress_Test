/// RTP 品質統計
/// 實作 RFC 3550 §A.8 jitter 計算 + E-Model MOS 估算

use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

// ── 即時統計（原子，無鎖）────────────────────────────────────────

#[derive(Default)]
pub struct RtpStats {
    /// 已傳送 RTP 封包數
    pub sent_packets:  AtomicU64,
    /// 已傳送 bytes
    pub sent_bytes:    AtomicU64,
    /// 已接收 RTP 封包數
    pub recv_packets:  AtomicU64,
    /// 累計 jitter（µs × 16，RFC 3550 格式）
    jitter_x16_us:    AtomicU64,
    /// 上一個封包的 RTP 時間戳記
    last_rtp_ts:      Mutex<Option<u32>>,
    /// 上一個封包的到達時間（µs）
    last_recv_us:     Mutex<Option<u64>>,
    /// 上一個封包的序號（用於 jitter / 亂序偵測）
    last_seq:         Mutex<Option<u16>>,
    /// 接收到的第一個序號（用於計算 expected，RFC 3550 §A.3）
    first_seq:        Mutex<Option<u16>>,
    /// 接收到的最高序號
    max_seq:          Mutex<Option<u16>>,
    /// 序號 wrap-around 次數（每次 u16 回繞 +1）
    seq_cycles:       AtomicU64,
    /// 累計亂序封包數
    pub out_of_order: AtomicU64,
    /// 累計重複封包數
    pub duplicates:   AtomicU64,
}

impl RtpStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 記錄傳送事件
    pub fn on_send(&self, payload_size: usize) {
        self.sent_packets.fetch_add(1, Ordering::Relaxed);
        self.sent_bytes.fetch_add(payload_size as u64, Ordering::Relaxed);
    }

    /// 記錄接收事件（rtp_ts = 封包內時間戳記，recv_us = 系統時間 µs）
    pub fn on_recv(&self, seq: u16, rtp_ts: u32, recv_us: u64) {
        self.recv_packets.fetch_add(1, Ordering::Relaxed);

        // ── Jitter 計算（RFC 3550 §A.8）──
        let mut last_ts  = self.last_rtp_ts.lock().unwrap();
        let mut last_us  = self.last_recv_us.lock().unwrap();
        let mut last_seq = self.last_seq.lock().unwrap();

        if let (Some(prev_ts), Some(prev_us), Some(prev_seq)) =
            (*last_ts, *last_us, *last_seq)
        {
            let seq_diff = seq.wrapping_sub(prev_seq);
            if seq_diff == 0 {
                self.duplicates.fetch_add(1, Ordering::Relaxed);
            } else if seq_diff > 0x7FFF {
                // 亂序（回退）
                self.out_of_order.fetch_add(1, Ordering::Relaxed);
            } else {
                // 正常或略微超前，計算 jitter
                let recv_diff_8k = ((recv_us.wrapping_sub(prev_us)) as f64
                    / 1_000_000.0 * 8000.0) as i64;
                let rtp_diff = rtp_ts.wrapping_sub(prev_ts) as i64;
                let d = (recv_diff_8k - rtp_diff).unsigned_abs() as u64;
                let j = self.jitter_x16_us.load(Ordering::Relaxed);
                let new_j = j.saturating_add(d).saturating_sub(j / 16);
                self.jitter_x16_us.store(new_j, Ordering::Relaxed);
            }
        }

        *last_ts  = Some(rtp_ts);
        *last_us  = Some(recv_us);
        *last_seq = Some(seq);

        // ── 序號追蹤（RFC 3550 §A.3 掉包率基礎）──
        let mut first_seq = self.first_seq.lock().unwrap();
        let mut max_seq   = self.max_seq.lock().unwrap();

        if first_seq.is_none() {
            *first_seq = Some(seq);
            *max_seq   = Some(seq);
        } else if let Some(prev_max) = *max_seq {
            let diff = seq.wrapping_sub(prev_max);
            // diff in (0, 0x7FFF] → 序號前進
            if diff > 0 && diff <= 0x7FFF {
                // 若發生 u16 wrap-around（新 seq < prev_max 且差值 > 1 month）
                if seq < prev_max && diff < 0x8000 {
                    self.seq_cycles.fetch_add(1, Ordering::Relaxed);
                }
                *max_seq = Some(seq);
            }
        }
    }

    /// Jitter（ms）
    pub fn jitter_ms(&self) -> f64 {
        let j_8k = self.jitter_x16_us.load(Ordering::Relaxed) as f64 / 16.0;
        j_8k / 8.0  // 8kHz 單位 → ms
    }

    /// 掉包率（0.0 ~ 1.0）
    /// 依接收端序號空間計算：(expected - received) / expected（RFC 3550 §A.3）
    pub fn packet_loss_rate(&self) -> f64 {
        let recv     = self.recv_packets.load(Ordering::Relaxed);
        let first    = *self.first_seq.lock().unwrap();
        let max      = *self.max_seq.lock().unwrap();
        let cycles   = self.seq_cycles.load(Ordering::Relaxed);

        match (first, max) {
            (Some(f), Some(m)) => {
                // expected = cycles * 65536 + (max - first + 1)
                let span: u64 = cycles * 65536 + m.wrapping_sub(f) as u64 + 1;
                if span == 0 { return 0.0; }
                let lost = span.saturating_sub(recv);
                lost as f64 / span as f64
            }
            _ => 0.0,
        }
    }

    /// 快照
    pub fn snapshot(&self) -> RtpStatsSnapshot {
        let sent      = self.sent_packets.load(Ordering::Relaxed);
        let recv      = self.recv_packets.load(Ordering::Relaxed);
        let first     = *self.first_seq.lock().unwrap();
        let max       = *self.max_seq.lock().unwrap();
        let cycles    = self.seq_cycles.load(Ordering::Relaxed);

        let (expected, lost) = match (first, max) {
            (Some(f), Some(m)) => {
                let span = cycles * 65536 + m.wrapping_sub(f) as u64 + 1;
                (span, span.saturating_sub(recv))
            }
            _ => (0, 0),
        };

        let loss_rate = if expected == 0 { 0.0 } else { lost as f64 / expected as f64 };
        let jitter_ms = self.jitter_ms();
        let mos       = estimate_mos(loss_rate, jitter_ms);

        RtpStatsSnapshot {
            sent_packets:  sent,
            recv_packets:  recv,
            lost_packets:  lost,
            loss_rate_pct: loss_rate * 100.0,
            jitter_ms,
            mos,
            out_of_order:  self.out_of_order.load(Ordering::Relaxed),
            duplicates:    self.duplicates.load(Ordering::Relaxed),
        }
    }
}

// ── 快照（可序列化）────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RtpStatsSnapshot {
    pub sent_packets:  u64,
    pub recv_packets:  u64,
    pub lost_packets:  u64,
    /// 掉包率（%）
    pub loss_rate_pct: f64,
    /// Jitter（ms）
    pub jitter_ms:     f64,
    /// MOS 估算值（1.0 ~ 5.0）
    pub mos:           f64,
    pub out_of_order:  u64,
    pub duplicates:    u64,
}

impl RtpStatsSnapshot {
    pub fn mos_label(&self) -> &'static str {
        mos_label(self.mos)
    }
}

// ── MOS 估算（ITU-T E-Model 簡化版）────────────────────────────

/// 由掉包率（0.0~1.0）和 jitter（ms）估算 MOS（1.0~5.0）
pub fn estimate_mos(loss_rate: f64, jitter_ms: f64) -> f64 {
    let r0  = 93.2_f64;
    let ppl = (loss_rate * 100.0).min(100.0);
    let ie  = 0.0_f64;
    let bpl = 4.3_f64;
    let ie_eff = ie + (95.0 - ie) * ppl / (ppl + bpl);
    let id = if jitter_ms < 150.0 { 0.0 } else { (jitter_ms - 150.0) * 0.1 };
    let r  = (r0 - ie_eff - id).max(0.0).min(100.0);
    r_to_mos(r)
}

/// R 值 → MOS（ITU-T G.107 §B.4）
fn r_to_mos(r: f64) -> f64 {
    if r <= 0.0 { return 1.0; }
    if r >= 100.0 { return 4.5; }
    let mos = 1.0 + 0.035 * r + r * (r - 60.0) * (100.0 - r) * 7e-6;
    mos.max(1.0).min(5.0)
}

/// MOS 文字等級
pub fn mos_label(mos: f64) -> &'static str {
    match mos as u32 {
        5 => "優秀 (Excellent)",
        4 => "良好 (Good)",
        3 => "普通 (Fair)",
        2 => "差  (Poor)",
        _ => "劣  (Bad)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mos_perfect() {
        let mos = estimate_mos(0.0, 0.0);
        assert!(mos > 4.0, "MOS={}", mos);
    }

    #[test]
    fn test_mos_high_loss() {
        let mos = estimate_mos(0.20, 20.0);
        assert!(mos < 2.5, "MOS={}", mos);
    }

    #[test]
    fn test_mos_range() {
        let mos = estimate_mos(0.05, 80.0);
        assert!(mos >= 1.0 && mos <= 5.0, "MOS={} out of range", mos);
    }

    #[test]
    fn test_loss_rate_seq_based() {
        let stats = RtpStats::new();
        // 模擬收到 seq 100, 101, 103（跳過 102 = 1 個掉包）
        let t = 0u64;
        stats.on_recv(100, 0, t);
        stats.on_recv(101, 160, t + 20_000);
        stats.on_recv(103, 480, t + 60_000);
        // expected = 103 - 100 + 1 = 4, recv = 3, lost = 1
        let loss = stats.packet_loss_rate();
        assert!((loss - 0.25).abs() < 0.01, "loss={}", loss);
    }
}
