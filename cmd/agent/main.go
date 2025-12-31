package main

import (
	"Sipx_Stress_Test/internal"
	"Sipx_Stress_Test/internal/config"
	"flag"
	"log"
)

var logFile = flag.String("log-file", "logs/sipload.log", "Path to log file")

func main() {
	flag.Parse()
	f, err := logger.Init(*logFile)
	if err != nil {
		log.Fatal(err)
	}
	defer f.Close()
	cfg, err := config.Load("Config.yaml")
	if err != nil {
		logger.Error("Failed to load config:", err)
		return
	}
	logger.Info("Config loaded successfully")
	//驗證agent數量
	if cfg.Agent.Count <= 0 {
		logger.Error("Agent count (%d) must be greater than zero", cfg.Agent.Count)
		return
	}
}
