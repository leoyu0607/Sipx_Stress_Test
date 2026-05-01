@echo off
:: sipress build launcher - calls build.ps1
::
:: CLI targets:
::   build.bat linux-x86        Linux x86_64 static (musl)
::   build.bat linux-arm64      Linux ARM64 static (musl)
::   build.bat windows          Windows x86_64 (GNU, via zigbuild)
::   build.bat windows-native   Windows x86_64 (MSVC, fastest, no zig)
::   build.bat macos-x86        macOS x86_64 (requires macOS SDK)
::   build.bat macos-arm64      macOS ARM64 (requires macOS SDK)
::
:: GUI target:
::   build.bat gui              Tauri GUI app (requires node/npm)
::
:: Combined:
::   build.bat all              Linux x86+arm64, Windows GNU+native, GUI

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
