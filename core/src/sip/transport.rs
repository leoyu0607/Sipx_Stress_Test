/// UDP / TCP 傳輸層封裝
use anyhow::{Context, Result};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tracing::debug;

/// 共用 UDP socket（Arc 包裹，多 task 共享）
pub struct SharedUdpSocket {
    pub socket:     UdpSocket,
    pub local_addr: String,
    pub server:     SocketAddr,
}

impl SharedUdpSocket {
    /// 建立並綁定 UDP socket，設定預設遠端地址
    pub async fn new(server: SocketAddr, local_ip: &str) -> Result<Self> {
        // 取出純 IP（去掉 port 部分）
        let ip = local_ip.split(':').next().unwrap_or("0.0.0.0");
        let bind_addr = format!("{}:0", ip);

        let socket = UdpSocket::bind(&bind_addr)
            .await
            .with_context(|| format!("無法綁定 UDP socket: {}", bind_addr))?;

        // 設定預設傳送目標（讓 send() 不需要每次指定 addr）
        socket.connect(server).await
            .with_context(|| format!("無法連接至 SIP 伺服器: {}", server))?;

        let local_addr = socket.local_addr()
            .context("無法取得本機地址")?
            .to_string();

        debug!("UDP socket 已綁定: {} → {}", local_addr, server);

        Ok(Self { socket, local_addr, server })
    }

    /// 傳送 SIP 訊息（raw text）
    pub async fn send(&self, msg: &str) -> Result<()> {
        let bytes = msg.as_bytes();
        self.socket.send(bytes).await
            .with_context(|| "UDP 傳送失敗")?;
        debug!("> {} bytes", bytes.len());
        Ok(())
    }
}
