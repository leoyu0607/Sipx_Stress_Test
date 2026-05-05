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
    ) -> String {
        format!(
            "ACK sip:{to}@{server} SIP/2.0\r\n\
             Via: SIP/2.0/{transport} {local};branch={branch}\r\n\
             Max-Forwards: 70\r\n\
             From: <sip:{from}@{from_domain}>;tag={from_tag}\r\n\
             To: <sip:{to}@{server}>;tag={to_tag}\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {cseq} ACK\r\n\
             Content-Length: 0\r\n\
             \r\n",
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
    ) -> String {
        format!(
            "BYE sip:{to}@{server} SIP/2.0\r\n\
             Via: SIP/2.0/{transport} {local};branch={branch}\r\n\
             Max-Forwards: 70\r\n\
             From: <sip:{from}@{from_domain}>;tag={from_tag}\r\n\
             To: <sip:{to}@{server}>;tag={to_tag}\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {cseq} BYE\r\n\
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
    /// 固定使用 G.711A（PCMA，PT=8）作為媒體 codec，
    /// 伺服器通常以 offer 中第一個 codec 為主，明確指定避免協商錯誤。
    fn minimal_sdp(local_ip: &str, rtp_port: u16) -> String {
        let ip = local_ip.split(':').next().unwrap_or(local_ip);
        format!(
            "v=0\r\n\
             o=sipress 1000 1000 IN IP4 {ip}\r\n\
             s=sipress\r\n\
             c=IN IP4 {ip}\r\n\
             t=0 0\r\n\
             m=audio {port} RTP/AVP 8\r\n\
             a=rtpmap:8 PCMA/8000\r\n\
             a=ptime:20\r\n\
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

/// 建構對伺服器發來請求的 200 OK 回應（RE-INVITE / BYE）
impl SipMessage {
    /// 從請求訊息中擷取必要標頭（Via / From / To / Call-ID / CSeq）
    fn extract_request_headers(raw: &str) -> (String, String, String, String, String) {
        let (mut vias, mut from, mut to, mut call_id, mut cseq) =
            (Vec::<String>::new(), String::new(), String::new(), String::new(), String::new());
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
        let via = vias.join("\r\n");
        (via, from, to, call_id, cseq)
    }

    /// 對伺服器發來的 BYE 回應 200 OK（不含 SDP）
    pub fn ok_for_server_bye(raw_request: &str) -> String {
        let (via, from, to, call_id, cseq) = Self::extract_request_headers(raw_request);
        format!(
            "SIP/2.0 200 OK\r\n\
             {via}\r\n\
             {from}\r\n\
             {to}\r\n\
             {call_id}\r\n\
             {cseq}\r\n\
             Content-Length: 0\r\n\
             \r\n",
            via = via, from = from, to = to, call_id = call_id, cseq = cseq,
        )
    }

    /// 對伺服器發來的 RE-INVITE 回應 200 OK（含 SDP，維持 PCMA 通話）
    pub fn ok_for_server_reinvite(raw_request: &str, local_addr: &str, rtp_port: u16) -> String {
        let (via, from, to, call_id, cseq) = Self::extract_request_headers(raw_request);
        let sdp     = Self::minimal_sdp(local_addr, rtp_port);
        let sdp_len = sdp.len();
        format!(
            "SIP/2.0 200 OK\r\n\
             {via}\r\n\
             {from}\r\n\
             {to}\r\n\
             {call_id}\r\n\
             {cseq}\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {sdp_len}\r\n\
             \r\n\
             {sdp}",
            via     = via,
            from    = from,
            to      = to,
            call_id = call_id,
            cseq    = cseq,
            sdp_len = sdp_len,
            sdp     = sdp,
        )
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

    /// 從 200 OK 的 SDP body 中解析對端 RTP 地址（IP:port）
    /// 同時解析 c= connection line 與 m=audio port，回傳 "ip:port" 字串。
    /// 若 c= 不存在，以 fallback_ip（SIP server IP）代替。
    pub fn sdp_rtp_addr(raw: &str, fallback_ip: &str) -> Option<String> {
        let body_start = raw.find("\r\n\r\n").map(|i| i + 4)
            .or_else(|| raw.find("\n\n").map(|i| i + 2))?;
        let body = &raw[body_start..];

        // 解析 c= line（例：c=IN IP4 192.168.1.10）
        let mut conn_ip = fallback_ip.to_string();
        // 先掃一輪拿 session-level c=
        for line in body.lines() {
            let line = line.trim();
            if line.starts_with("c=") {
                // c=IN IP4 <ip>  或  c=IN IP6 <ip>
                let parts: Vec<&str> = line.splitn(4, ' ').collect();
                if parts.len() >= 3 {
                    let ip = parts[2].trim().trim_end_matches('\r');
                    if !ip.is_empty() && ip != "0.0.0.0" {
                        conn_ip = ip.to_string();
                    }
                }
                break; // 取第一個 c= (session level)
            }
        }

        // 解析 m=audio port（取第一個 audio m= 行）
        let mut rtp_port: Option<u16> = None;
        let mut in_audio_section = false;
        for line in body.lines() {
            let line = line.trim();
            if line.starts_with("m=") {
                in_audio_section = line.starts_with("m=audio");
                if in_audio_section {
                    let parts: Vec<&str> = line.splitn(4, ' ').collect();
                    if parts.len() >= 2 {
                        if let Ok(p) = parts[1].parse::<u16>() {
                            rtp_port = Some(p);
                        }
                    }
                }
            }
            // media-level c= 覆蓋 session-level（取 audio section 內的 c=）
            if in_audio_section && line.starts_with("c=") {
                let parts: Vec<&str> = line.splitn(4, ' ').collect();
                if parts.len() >= 3 {
                    let ip = parts[2].trim().trim_end_matches('\r');
                    if !ip.is_empty() && ip != "0.0.0.0" {
                        conn_ip = ip.to_string();
                    }
                }
            }
            if rtp_port.is_some() && in_audio_section {
                break; // audio section 處理完畢
            }
        }

        rtp_port.map(|p| format!("{}:{}", conn_ip, p))
    }

    /// 從 200 OK 的 SDP body 中解析對端 RTP port（向下相容用，只取 port）
    pub fn sdp_rtp_port(raw: &str) -> Option<u16> {
        let body_start = raw.find("\r\n\r\n").map(|i| i + 4)
            .or_else(|| raw.find("\n\n").map(|i| i + 2))?;
        let body = &raw[body_start..];
        for line in body.lines() {
            if line.trim().starts_with("m=audio") || line.trim().starts_with("m=") {
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
