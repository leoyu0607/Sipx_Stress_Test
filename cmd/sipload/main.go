package main

import (
	"Sipx_Stress_Test/internal"
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
	logger.Info("Hello World")
}
