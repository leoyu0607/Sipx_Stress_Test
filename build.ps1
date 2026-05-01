# sipress Windows build script (PowerShell)
# Supports cargo-zigbuild cross-compilation + native Windows MSVC build
# Usage: .\build.ps1 [linux-x86|linux-arm64|windows|windows-native|macos-x86|macos-arm64|all]
param(
    [string]$Target = "all"
)

$ErrorActionPreference = "Stop"
$BIN   = "sipress"
$OUT   = "dist"
$CARGO = "cli\Cargo.toml"

# ── Output helpers ─────────────────────────────────────────────────
function Info ([string]$msg) { Write-Host "[sipress] $msg" -ForegroundColor Cyan  }
function Ok   ([string]$msg) { Write-Host "[  OK  ] $msg"  -ForegroundColor Green }
function Err  ([string]$msg) { Write-Host "[ERROR ] $msg"  -ForegroundColor Red; exit 1 }

# ── Auto-detect zig from Python ziglang package ───────────────────
function Ensure-Zig {
    if (Get-Command zig -ErrorAction SilentlyContinue) { return }

    $zigDir = python -c "import ziglang, os; print(os.path.dirname(ziglang.__file__))" 2>$null
    if ($zigDir -and (Test-Path "$zigDir\zig.exe")) {
        $env:PATH = "$zigDir;$env:PATH"
        $ver = & "$zigDir\zig.exe" version
        Info "Found zig via ziglang package: $zigDir\zig.exe ($ver)"
    } else {
        Err "zig not found. Install: pip install ziglang  OR  download from ziglang.org"
    }
}

function Ensure-Zigbuild {
    if (-not (Get-Command cargo-zigbuild -ErrorAction SilentlyContinue)) {
        Err "cargo-zigbuild not found. Install: cargo install cargo-zigbuild"
    }
}

# ── Build functions ────────────────────────────────────────────────
function Build-CrossTarget ([string]$rustTarget, [string]$outName) {
    Info "Cross-compiling $rustTarget ..."
    $null = New-Item -ItemType Directory -Path $OUT -Force

    cargo zigbuild --manifest-path $CARGO --target $rustTarget --release -q
    if ($LASTEXITCODE -ne 0) { Err "Build failed for $rustTarget" }

    $ext = if ($outName -like "*.exe") { ".exe" } else { "" }
    $src = "target\$rustTarget\release\$BIN$ext"
    Copy-Item -Path $src -Destination "$OUT\$outName" -Force
    $mb = [math]::Round((Get-Item "$OUT\$outName").Length / 1MB, 2)
    Ok "$OUT\$outName  ($mb MB)"
}

function Build-NativeWindows {
    Info "Native Windows build (x86_64-pc-windows-msvc, no zigbuild needed) ..."
    $null = New-Item -ItemType Directory -Path $OUT -Force

    cargo build --manifest-path $CARGO --release -q
    if ($LASTEXITCODE -ne 0) { Err "Native Windows build failed" }

    $src = "target\release\$BIN.exe"
    $dst = "$OUT\$BIN-windows-x86_64-native.exe"
    Copy-Item -Path $src -Destination $dst -Force
    $mb = [math]::Round((Get-Item $dst).Length / 1MB, 2)
    Ok "$dst  ($mb MB)"
}

# ── Main ───────────────────────────────────────────────────────────
Info "Target: $Target"

switch ($Target.ToLower()) {
    "linux-x86" {
        Ensure-Zig; Ensure-Zigbuild
        rustup target add x86_64-unknown-linux-musl --quiet
        Build-CrossTarget "x86_64-unknown-linux-musl" "$BIN-linux-x86_64"
    }
    "linux-arm64" {
        Ensure-Zig; Ensure-Zigbuild
        rustup target add aarch64-unknown-linux-musl --quiet
        Build-CrossTarget "aarch64-unknown-linux-musl" "$BIN-linux-arm64"
    }
    "windows" {
        Ensure-Zig; Ensure-Zigbuild
        rustup target add x86_64-pc-windows-gnu --quiet
        Build-CrossTarget "x86_64-pc-windows-gnu" "$BIN-windows-x86_64.exe"
    }
    "windows-native" {
        Build-NativeWindows
    }
    "macos-x86" {
        Ensure-Zig; Ensure-Zigbuild
        rustup target add x86_64-apple-darwin --quiet
        Build-CrossTarget "x86_64-apple-darwin" "$BIN-macos-x86_64"
    }
    "macos-arm64" {
        Ensure-Zig; Ensure-Zigbuild
        rustup target add aarch64-apple-darwin --quiet
        Build-CrossTarget "aarch64-apple-darwin" "$BIN-macos-arm64"
    }
    "all" {
        Ensure-Zig; Ensure-Zigbuild
        # Ensure all cross-compile targets are installed
        rustup target add x86_64-unknown-linux-musl  --quiet
        rustup target add aarch64-unknown-linux-musl --quiet
        rustup target add x86_64-pc-windows-gnu      --quiet
        Build-CrossTarget "x86_64-unknown-linux-musl"  "$BIN-linux-x86_64"
        Build-CrossTarget "aarch64-unknown-linux-musl" "$BIN-linux-arm64"
        Build-CrossTarget "x86_64-pc-windows-gnu"      "$BIN-windows-x86_64.exe"
        Build-NativeWindows
    }
    default {
        Write-Host "Usage: .\build.ps1 [linux-x86|linux-arm64|windows|windows-native|macos-x86|macos-arm64|all]"
        Write-Host ""
        Write-Host "  linux-x86        Linux x86_64 static (musl)"
        Write-Host "  linux-arm64      Linux ARM64 static (musl)"
        Write-Host "  windows          Windows x86_64 (GNU, via zigbuild)"
        Write-Host "  windows-native   Windows x86_64 (MSVC, native, fastest)"
        Write-Host "  macos-x86        macOS x86_64 (requires macOS SDK)"
        Write-Host "  macos-arm64      macOS ARM64 (requires macOS SDK)"
        Write-Host "  all              Linux x86+arm64, Windows GNU+native"
        exit 1
    }
}

Info "Output dir: $OUT\"
Get-ChildItem $OUT | ForEach-Object {
    $mb = [math]::Round($_.Length / 1MB, 2)
    Write-Host ("  {0,-48} {1,6} MB" -f $_.Name, $mb)
}
