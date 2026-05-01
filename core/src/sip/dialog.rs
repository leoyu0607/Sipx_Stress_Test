/// SIP 對話狀態機
use std::time::Instant;
use serde::{Deserialize, Serialize};

/// 單一通話的完整狀態
#[derive(Debug, Clone)]
pub struct Dialog {
    pub call_id:    String,
    pub from_tag:   String,
    pub to_tag:     Option<String>,   // 收到 200 OK 後才有
    pub branch:     String,
    pub cseq:       u32,
    pub state:      DialogState,

    // 計時
    pub invite_sent_at:  Instant,
    pub ringing_at:      Option<Instant>,  // 收到 180
    pub answered_at:     Option<Instant>,  // 收到 200 OK
    pub bye_sent_at:     Option<Instant>,
    pub ended_at:        Option<Instant>,

    // 目標號碼（記錄用）
    pub callee: String,

    // 本機 RTP port（INVITE 前預分配，用於 SDP 與 RTP session）
    pub local_rtp_port: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DialogState {
    /// INVITE 已送出，等待回應
    Calling,
    /// 收到 100 Trying
    Trying,
    /// 收到 180 Ringing，PDD 計時結束
    Ringing,
    /// 收到 200 OK，ACK 已送出
    Connected,
    /// BYE 已送出，等待 200 OK
    Terminating,
    /// 通話正常結束
    Completed,
    /// 被拒絕（4xx/5xx/6xx）
    Failed(u16),
    /// 逾時（未收到回應）
    TimedOut,
    /// 主動 CANCEL
    Cancelled,
}

impl Dialog {
    pub fn new(
        call_id: String,
        from_tag: String,
        branch: String,
        callee: String,
        local_rtp_port: u16,
    ) -> Self {
        Self {
            call_id,
            from_tag,
            to_tag:       None,
            branch,
            cseq:         1,
            state:        DialogState::Calling,
            invite_sent_at: Instant::now(),
            ringing_at:   None,
            answered_at:  None,
            bye_sent_at:  None,
            ended_at:     None,
            callee,
            local_rtp_port,
        }
    }

    /// 收到 100 Trying
    pub fn on_trying(&mut self) {
        if self.state == DialogState::Calling {
            self.state = DialogState::Trying;
        }
    }

    /// 收到 180 Ringing → 記錄 PDD
    pub fn on_ringing(&mut self) {
        if matches!(self.state, DialogState::Calling | DialogState::Trying) {
            self.state     = DialogState::Ringing;
            self.ringing_at = Some(Instant::now());
        }
    }

    /// 收到 200 OK → 記錄通話建立時間，準備送 ACK
    pub fn on_ok(&mut self, to_tag: String) {
        if matches!(self.state, DialogState::Calling | DialogState::Trying | DialogState::Ringing) {
            self.state       = DialogState::Connected;
            self.to_tag      = Some(to_tag);
            self.answered_at = Some(Instant::now());
        }
    }

    /// 準備送 BYE
    pub fn on_bye_sent(&mut self) {
        self.state       = DialogState::Terminating;
        self.bye_sent_at = Some(Instant::now());
        self.cseq       += 1;
    }

    /// 收到 BYE 的 200 OK
    pub fn on_bye_ok(&mut self) {
        self.state    = DialogState::Completed;
        self.ended_at = Some(Instant::now());
    }

    /// 收到錯誤回應
    pub fn on_error(&mut self, code: u16) {
        self.state    = DialogState::Failed(code);
        self.ended_at = Some(Instant::now());
    }

    /// 逾時
    pub fn on_timeout(&mut self) {
        self.state    = DialogState::TimedOut;
        self.ended_at = Some(Instant::now());
    }

    // ── 指標計算 ──

    /// PDD（Post Dial Delay）：INVITE → 180 Ringing（毫秒）
    pub fn pdd_ms(&self) -> Option<f64> {
        self.ringing_at.map(|r| {
            r.duration_since(self.invite_sent_at).as_secs_f64() * 1000.0
        })
    }

    /// 通話建立時間：INVITE → 200 OK（毫秒）
    pub fn setup_time_ms(&self) -> Option<f64> {
        self.answered_at.map(|a| {
            a.duration_since(self.invite_sent_at).as_secs_f64() * 1000.0
        })
    }

    /// 通話持續時間：200 OK → BYE 200 OK（秒）
    pub fn call_duration_secs(&self) -> Option<f64> {
        match (self.answered_at, self.ended_at) {
            (Some(a), Some(e)) if self.state == DialogState::Completed => {
                Some(e.duration_since(a).as_secs_f64())
            }
            _ => None,
        }
    }

    /// 是否成功接通
    pub fn is_answered(&self) -> bool {
        self.answered_at.is_some()
    }
}
