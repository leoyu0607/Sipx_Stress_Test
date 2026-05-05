/// SIP 訊息建構器
/// 手刻 SIP RFC 3261 格式，不依賴外部複雜 crate，靜態編譯友好
use uuid::Uuid;

pub struct SipMessage;

impl SipMessage {
    /// 建構 INVITE 請求
    pub fn invite(
        call_id:      &str,
        from_number:  &str,
        from_domain:  &str,
        to_number:    &str,
        server_addr:  &str,
        local_addr:   &str,
        cseq:         u32,
        branch:       &str,
        tag:          &str,
        transport:    &str,   // "UDP" or "TCP"
        rtp_port:     u16,    // 本機 RTP port（寫入 SDP m= 行）
    ) -> String {
        let sdp = Self::minimal_sdp(local_addr, rtp_port);
        let sdp_len = sdp.len();

        format!(
            "INVITE sip:{to}@{server} SIP/2.0\r\n\
             Via: SIP/2.0/{transport} {local};branch={branch};rport\r\n\
             Max-Forwards: 70\r\n\
             From: <sip:{from}@{from_domain}>;tag={tag}\r\n\
             To: <sip:{to}@{server}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {cseq} INVITE\r\n\
             Contact: <sip:{from}@{local};transport={transport_lower}>\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {sdp_len}\r\n\
             User-Agent: sipress/0.1\r\n\
             Allow: INVITE,ACK,BYE,CANCEL,OPTIONS\r\n\
             \r\n\
             {sdp}",
            to           = to_number,
            server       = server_addr,
            transport    = transport,
            transport_lower = transport.to_lowercase(),
            local        = local_addr,
            branch       = branch,
            from         = from_number,
            from_domain  = from_domain,
            tag          = tag,
            call_id      = call_id,
            cseq         = cseq,
            sdp_len      = sdp_len,
            sdp          = sdp,
        )
    }

    /// 建構 ACK（收到 200 OK 後送出）
    ///
    /// `request_uri`: 2xx ACK 應使用 Contact URI（RFC 3261 §13.2.2.4）；
    ///                non-2xx ACK 傳 None，回退使用 sip:{to}@{server}。
    pub fn ack(
        call_id:     &str,
        from_number: &str,
        from_domain: &str,
        to_number:   &str,
        to_tag:      &str,
        server_addr: &str,
        local_addr:  &str,
        cseq:        u32,
        branch:      &str,
        from_tag:    &str,
        transport:   &str,
        request_uri: Option<&str>,
    ) -> String {
        let uri = request_uri
            .map(|u| u.to_string())
            .unwrap_or_else(|| format!("sip:{}@{}", to_number, server_addr));
        format!(
            "ACK {uri} SIP/2.0\r\n\
             Via: SIP/2.0/{transport} {local};branch={branch}\r\n\
             Max-Forwards: 70\r\n\
             From: <sip:{from}@{from_domain}>;tag={from_tag}\r\n\
             To: <sip:{to}@{server}>;tag={to_tag}\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {cseq} ACK\r\n\
             Content-Length: 0\r\n\
             \r\n",
            uri        = uri,
            to         = to_number,
            server     = server_addr,
            transport  = transport,
            local      = local_addr,
            branch     = branch,
            from       = from_number,
            from_domain = from_domain,
            from_tag   = from_tag,
            to_tag     = to_tag,
            call_id    = call_id,
            cseq       = cseq,
        )
    }

    /// 建構 BYE
    ///
    /// `request_uri`: 應傳入 200 OK Contact URI（RFC 3261 §12.2.1.1）；
    ///                None 時回退使用 sip:{to}@{server}。
    pub fn bye(
        call_id:     &str,
        from_number: &str,
        from_domain: &str,
        to_number:   &str,
        to_tag:      &str,
        server_addr: &str,
        local_addr:  &str,
        cseq:        u32,
        branch:      &str,
        from_tag:    &str,
        transport:   &str,
        request_uri: Option<&str>,
    ) -> String {
        let uri = request_uri
            .map(|u| u.to_string())
            .unwrap_or_else(|| format!("sip:{}@{}", to_number, server_addr));
        format!(
            "BYE {uri} SIP/2.0\r\n\
             Via: SIP/2.0/{transport} {local};branch={branch}\r\n\
             Max-Forwards: 70\r\n\
             From: <sip:{from}@{from_domain}>;tag={from_tag}\r\n\
             To: <sip:{to}@{server}>;tag={to_tag}\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {cseq} BYE\r\n\
             Content-Length: 0\r\n\
             \r\n",
            uri         = uri,
            to          = to_number,
            server      = server_addr,
            transport   = transport,
            local       = local_addr,
            branch      = branch,
            from        = from_number,
            from_domain = from_domain,
            from_tag    = from_tag,
            to_tag      = to_tag,
            call_id     = call_id,
            cseq        = cseq,
        )
    }

    /// 建構 CANCEL
    pub fn cancel(
        call_id:     &str,
        from_number: &str,
        from_domain: &str,
        to_number:   &str,
        server_addr: &str,
        local_addr:  &str,
        cseq:        u32,
        branch:      &str,
        from_tag:    &str,
        transport:   &str,
    ) -> String {
        format!(
            "CANCEL sip:{to}@{server} SIP/2.0\r\n\
             Via: SIP/2.0/{transport} {local};branch={branch}\r\n\
             Max-Forwards: 70\r\n\
             From: <sip:{from}@{from_domain}>;tag={from_tag}\r\n\
             To: <sip:{to}@{server}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {cseq} CANCEL\r\n\
             Content-Length: 0\r\n\
             \r\n",
            to          = to_number,
            server      = server_addr,
            transport   = transport,
            local       = local_addr,
            branch      = branch,
            from        = from_number,
            from_domain = from_domain,
            from_tag    = from_tag,
            call_id     = call_id,
            cseq        = cseq,
        )
    }

    /// 最小 SDP
    fn minimal_sdp(local_ip: &str, rtp_port: u16) -> String {
        let ip = local_ip.split(':').next().unwrap_or(local_ip);
        format!(
            "v=0\r\n\
             o=sipress 1000 1000 IN IP4 {ip}\r\n\
             s=sipress\r\n\
             c=IN IP4 {ip}\r\n\
             t=0 0\r\n\
             m=audio {port} RTP/AVP 0 8\r\n\
             a=rtpmap:0 PCMU/8000\r\n\
             a=rtpmap:8 PCMA/8000\r\n\
             a=sendrecv\r\n",
            ip = ip,
            port = rtp_port,
        )
    }

    /// 產生 branch 參數（RFC 3261 要求以 z9hG4bK 開頭）
    pub fn new_branch() -> String {
        format!("z9hG4bK-{}", Uuid::new_v4().simple())
    }

    /// 產生 tag
    pub fn new_tag() -> String {
        Uuid::new_v4().simple().to_string()[..8].to_string()
    }

    /// 產生 Call-ID
    pub fn new_call_id(domain: &str) -> String {
        format!("{}@{}", Uuid::new_v4().simple(), domain)
    }
}

/// 解析 SIP 回應狀態碼與 To tag
pub struct SipResponse;

impl SipResponse {
    /// 從原始回應取得狀態碼（例如 200, 100, 180, 4xx, 5xx）
    pub fn status_code(raw: &str) -> Option<u16> {
        // SIP/2.0 200 OK
        let line = raw.lines().next()?;
        let mut parts = line.splitn(3, ' ');
        parts.next()?; // "SIP/2.0"
        parts.next()?.parse().ok()
    }

    /// 從 To header 取得 tag（200 OK 時軟交換機會加 To tag）
    pub fn to_tag(raw: &str) -> Option<String> {
        for line in raw.lines() {
            let lower = line.to_lowercase();
            if lower.starts_with("to:") || lower.starts_with("t:") {
                if let Some(pos) = lower.find(";tag=") {
                    let tag_start = pos + 5;
                    let tag = &line[tag_start..];
                    let tag = tag.split(';').next().unwrap_or(tag).trim();
                    return Some(tag.to_string());
                }
            }
        }
        None
    }

    /// 取得 CSeq method（用來辨別是哪個請求的回應）
    pub fn cseq_method(raw: &str) -> Option<String> {
        for line in raw.lines() {
            if line.to_lowercase().starts_with("cseq:") {
                let val = line[5..].trim();
                return val.split_whitespace().nth(1).map(|s| s.to_uppercase());
            }
        }
        None
    }

    /// 從 200 OK 的 SDP body 中解析對端 RTP port
    /// 格式：m=audio <port> RTP/AVP <fmt...>
    pub fn sdp_rtp_port(raw: &str) -> Option<u16> {
        // 找到 SIP header 與 SDP body 的分界（空行）
        let body_start = raw.find("\r\n\r\n").map(|i| i + 4)
            .or_else(|| raw.find("\n\n").map(|i| i + 2))?;
        let body = &raw[body_start..];
        for line in body.lines() {
            // m=audio PORT proto fmt  或  m=video PORT ...（取第一個 m= 行）
            if line.starts_with("m=") {
                let parts: Vec<&str> = line.splitn(4, ' ').collect();
                if parts.len() >= 2 {
                    if let Ok(port) = parts[1].parse::<u16>() {
                        return Some(port);
                    }
                }
            }
        }
        None
    }
}
