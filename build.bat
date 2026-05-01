@echo off
:: sipress build launcher - calls build.ps1
:: Usage: build.bat [linux-x86|linux-arm64|windows|windows-native|macos-x86|macos-arm64|all]
::
:: Examples:
::   build.bat windows-native   (fastest, no zig required)
::   build.bat linux-x86        (requires cargo-zigbuild + zig)
::   build.bat all              (Linux x86/arm64 + Windows)

setlocal EnableDelayedExpansion

if "%~1"=="" (
    set TARGET=all
) else (
    set TARGET=%~1
)

echo [sipress] Building target: %TARGET%

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" -Target "%TARGET%"

if %ERRORLEVEL% neq 0 (
    echo [ERROR] Build failed, exit code: %ERRORLEVEL%
    exit /b %ERRORLEVEL%
)
