package media

// G.711A的物理規則
const (
	rtpVersion    = 2 // RTP version 2 是目前唯一合法版本
	ptPCMA        = 8 // G.711 A-law
	clockRate     = 8000
	pktMs         = 20
	samplesPerPkt = clockRate * pktMs / 1000 // 160
	payloadLen    = samplesPerPkt            // G.711: 1 byte/sample
)
