package sip

type mode int

const (
	UAC mode = iota
	UAS
)

type SippJob struct {
	Mode       mode   // UAC or UAS
	Bin        string // sipp path
	Target     string // "10.0.0.1:5060"
	Scenario   string // "uac_inbound.xml" or "agent_register_and_answer.xml"
	ListenIP   string // UAS only
	ListenPort int    // UAS only
	RtpFile    string // RTP file path
	HoldMs     int    // RTP hold time in milliseconds
	InfCSV     string
	OutStat    string
	Rate       int
	Limit      int
	Calls      int
	Extra      []string // other flags
}
