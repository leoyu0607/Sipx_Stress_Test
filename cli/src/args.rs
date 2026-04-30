/// CLI 參數（clap derive）
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name    = "sipress",
    about   = "SIP 軟體交換機壓測工具",
    version = "0.1.0",
    long_about = None,
)]
pub struct Args {
    /// 目標 SIP 伺服器地址（格式：ip:port）
    #[arg(short = 's', long, default_value = "127.0.0.1:5060")]
    pub server: String,

    /// 最大並發通話數
    #[arg(short = 'c', long = "concurrent", default_value_t = 100)]
    pub max_concurrent: usize,

    /// 每秒通話數（CPS）
    #[arg(long, default_value_t = 10.0)]
    pub cps: f64,

    /// 壓測持續秒數
    #[arg(short = 'd', long = "duration", default_value_t = 60)]
    pub duration: u64,

    /// 單通通話持續秒數（0 = 不主動 BYE）
    #[arg(long = "call-duration", default_value_t = 30)]
    pub call_duration: u64,

    /// INVITE 逾時秒數
    #[arg(long = "invite-timeout", default_value_t = 8)]
    pub invite_timeout: u64,

    /// 主叫號碼
    #[arg(long = "from", default_value = "1000")]
    pub caller: String,

    /// 被叫號碼前綴
    #[arg(long = "to-prefix", default_value = "2")]
    pub callee_prefix: String,

    /// 被叫號碼尾數最大值（0..=N 隨機）
    #[arg(long = "to-range", default_value_t = 9999)]
    pub callee_range: u64,

    /// 本機綁定 IP（預設自動選擇）
    #[arg(long = "local")]
    pub local_addr: Option<String>,

    /// 本機 SIP domain
    #[arg(long = "domain")]
    pub local_domain: Option<String>,

    /// 啟用 TUI 即時儀表板
    #[arg(long)]
    pub tui: bool,

    /// 輸出格式：table（預設）/ json / csv
    #[arg(long = "format", default_value = "table")]
    pub format: String,

    /// 額外輸出 HTML 報告
    #[arg(long)]
    pub html: bool,

    /// HTML 報告輸出目錄
    #[arg(long = "report-dir", default_value = "reports")]
    pub report_dir: String,

    /// SIP log 輸出目錄
    #[arg(long = "logs-dir", default_value = "logs")]
    pub logs_dir: String,

    /// 啟用 RTP 音訊傳送
    #[arg(long = "rtp")]
    pub enable_rtp: bool,

    /// RTP 起始 port（偶數，預設 10000）
    #[arg(long = "rtp-port", default_value_t = 10000)]
    pub rtp_base_port: u16,

    /// 音檔路徑（.wav 或 .raw G.711 μ-law；未指定則靜音）
    #[arg(long = "audio")]
    pub audio_file: Option<std::path::PathBuf>,

    /// 顯示詳細 debug log
    #[arg(long)]
    pub verbose: bool,
}
