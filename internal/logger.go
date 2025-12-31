package logger

import (
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"runtime"
)

//var logFile = flag.String("log-file", "logs/sipload.log", "Path to log file")

func Init(path string) (*os.File, error) {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(
		path,
		os.O_CREATE|os.O_WRONLY|os.O_APPEND,
		0644,
	)
	if err != nil {
		return nil, err
	}
	mw := io.MultiWriter(os.Stdout, f)
	log.SetOutput(mw)
	log.SetFlags(log.Ldate | log.Ltime)
	return f, nil
}
func Info(format string, v ...any) {
	log.Printf("[INFO] "+format, v...)
}
func Debug(format string, v ...any) {
	log.Printf("[DEBUG] "+format, v...)
}
func Error(format string, v ...any) {
	_, file, line, ok := runtime.Caller(1)
	location := ""
	if ok {
		location = fmt.Sprintf("%s:%d ", filepath.Base(file), line)
	}
	log.Printf("[ERROR] %s"+format, append([]any{location}, v...)...)
}
