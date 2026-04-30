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
    pub sent_packets:     AtomicU64,
    /// 已傳送 bytes
    pub sent_bytes:       AtomicU64,
    /// 已接收 RTP 封包數
    pub recv_packets:     AtomicU64,
    /// 預期接收數（依序號推算）
    pub expected_packets: AtomicU64,
    /// 累計 jitter（µs × 16，RFC 3550 格式）
    jitter_x16_us: AtomicU64,
    /// 上一個封包的 RTP 時間戳記
    last_rtp_ts:   Mutex<Option<u32>>,
    /// 上一個封包的到達時間（µs）
    last_recv_us:  Mutex<Option<u64>>,
    /// 上一個封包的序號
    last_seq:      Mutex<Option<u16>>,
    /// 累計亂序封包數
    pub out_of_order:  AtomicU64,
    /// 累計重複封包數
    pub duplicates:    AtomicU64,
}

impl RtpStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 記錄傳送事件
    pub fn on_send(&self, payload_size: usize) {
        self.sent_packets.fetch_add(1, Ordering::Relaxed);
        self.sent_bytes.fetch_add(payload_size as u64, Ordering::Relaxed);
        self.expected_packets.fetch_add(1, Ordering::Relaxed);
    }

    /// 記錄接收事件（rtp_ts = 封包內時間戳記，recv_us = 系統時間 µs）
    pub fn on_recv(&self, seq: u16, rtp_ts: u32, recv_us: u64) {
        self.recv_packets.fetch_add(1, Ordering::Relaxed);

        // ── Jitter 計算（RFC 3550 §A.8）──
        // D(i,j) = |( Rj - Ri ) - ( Sj - Si )|（均以 8kHz clock 計）
        let mut last_ts  = self.last_rtp_ts.lock().unwrap();
        let mut last_us  = self.last_recv_us.lock().unwrap();
        let mut last_seq = self.last_seq.lock().unwrap();

        if let (Some(prev_ts), Some(prev_us), Some(prev_seq)) =
            (*last_ts, *last_us, *last_seq)
        {
            // 序號差（處理 wrap-around）
            let seq_diff = seq.wrapping_sub(prev_seq);
            if seq_diff == 0 {
                self.duplicates.fetch_add(1, Ordering::Relaxed);
            } else if seq_diff > 0x7FFF {
                // 亂序（回退）
                self.out_of_order.fetch_add(1, Ordering::Relaxed);
            } else {
                // 正常或略微超前
                // 收包時間差（µs）→ 換算成 8kHz clock 單位
                let recv_diff_8k = ((recv_us.wrapping_sub(prev_us)) as f64 / 1_000_000.0 * 8000.0) as i64;
                // RTP 時間戳記差
                let rtp_diff     = rtp_ts.wrapping_sub(prev_ts) as i64;
                // |D|（8kHz clock 單位）
                let d = (recv_diff_8k - rtp_diff).unsigned_abs() as u64;
                // 更新 jitter：J += ( |D| - J ) / 16（RFC 3550）
                let j = self.jitter_x16_us.load(Ordering::Relaxed);
                // 乘以 16 累積，避免浮點
                let new_j = j.saturating_add(d).saturating_sub(j / 16);
                self.jitter_x16_us.store(new_j, Ordering::Relaxed);
            }
        }

        *last_ts  = Some(rtp_ts);
        *last_us  = Some(recv_us);
        *last_seq = Some(seq);
    }

    /// Jitter（ms）
    pub fn jitter_ms(&self) -> f64 {
        // jitter_x16_us / 16 = jitter（8kHz units）→ ms
        let j_8k = self.jitter_x16_us.load(Ordering::Relaxed) as f64 / 16.0;
        j_8k / 8.0  // 8kHz 單位 → ms
    }

    /// 掉包率（0.0 ~ 1.0）
    pub fn packet_loss_rate(&self) -> f64 {
        let expected = self.expected_packets.load(Ordering::Relaxed);
        let recv     = self.recv_packets.load(Ordering::Relaxed);
        if expected == 0 { return 0.0; }
        let lost = expected.saturating_sub(recv);
        lost as f64 / expected as f64
    }

    /// 快照
    pub fn snapshot(&self) -> RtpStatsSnapshot {
        let sent     = self.sent_packets.load(Ordering::Relaxed);
        let recv     = self.recv_packets.load(Ordering::Relaxed);
        let expected = self.expected_packets.load(Ordering::Relaxed);
        let lost     = expected.saturating_sub(recv);
        let loss_rate = if expected == 0 { 0.0 } else { lost as f64 / expected as f64 };
        let jitter_ms = self.jitter_ms();
        let mos       = estimate_mos(loss_rate, jitter_ms);

        RtpStatsSnapshot {
            sent_packets:   sent,
            recv_packets:   recv,
            lost_packets:   lost,
            loss_rate_pct:  loss_rate * 100.0,
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
    /// MOS 等級描述
    pub fn mos_label(&self) -> &'static str {
        mos_label(self.mos)
    }
}

// ── MOS 估算（ITU-T E-Model 簡化版）────────────────────────────
//
// 完整 E-Model（ITU-T G.107）計算複雜，此處使用業界常用的簡化版本：
// 1. 計算 R 值（0~100）
// 2. R → MOS（ITU-T G.107 公式）
//
// 參考：
//   Teletraffic Engineering Handbook §9.4
//   Cisco Voice Over IP Design Guide

/// 由掉包率（0.0~1.0）和 jitter（ms）估算 MOS（1.0~5.0）
pub fn estimate_mos(loss_rate: f64, jitter_ms: f64) -> f64 {
    // 基礎 R 值（G.711 PCMU = 93.2）
    let r0 = 93.2_f64;

    // 掉包降分（Is）：使用 Vinke et al. 簡化模型
    // Ie_eff = Ie + (95 - Ie) × Ppl / (Ppl + Bpl)
    // G.711 的 Ie=0, Bpl=4.3
    let ppl   = (loss_rate * 100.0).min(100.0);
    let ie    = 0.0_f64;
    let bpl   = 4.3_f64;
    let ie_eff = ie + (95.0 - ie) * ppl / (ppl + bpl);

    // Jitter 降分（Id）：超過 150ms 開始顯著影響
    let id = if jitter_ms < 150.0 {
        0.0
    } else {
        (jitter_ms - 150.0) * 0.1
    };

    // R = R0 - Ie_eff - Id
    let r = (r0 - ie_eff - id).max(0.0).min(100.0);

    // R → MOS（ITU-T G.107）
    r_to_mos(r)
}

/// R 值 → MOS（ITU-T G.107 §B.4）
fn r_to_mos(r: f64) -> f64 {
    if r <= 0.0 { return 1.0; }
    if r >= 100.0 { return 4.5; }

    let mos = 1.0
        + 0.035 * r
        + r * (r - 60.0) * (100.0 - r) * 7e-6;

    mos.max(1.0).min(5.0)
}

/// MOS 文字等級
pub fn mos_label(mos: f64) -> &'static str {
    match mos as u32 {
        5            => "優秀 (Excellent)",
        4            => "良好 (Good)",
        3            => "普通 (Fair)",
        2            => "差  (Poor)",
        _            => "劣  (Bad)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mos_perfect() {
        // 0% 掉包 + 0ms jitter → 接近 4.4
        let mos = estimate_mos(0.0, 0.0);
        assert!(mos > 4.0, "MOS={}", mos);
    }

    #[test]
    fn test_mos_high_loss() {
        // 20% 掉包 → MOS < 2.5
        let mos = estimate_mos(0.20, 20.0);
        assert!(mos < 2.5, "MOS={}", mos);
    }

    #[test]
    fn test_mos_range() {
        let mos = estimate_mos(0.05, 80.0);
        assert!(mos >= 1.0 && mos <= 5.0, "MOS={} out of range", mos);
    }
}
