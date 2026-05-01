# sipress Windows build script — CLI (cargo-zigbuild) + GUI (Tauri)
# All outputs land in dist/
# Usage: .\build.ps1 [linux-x86|linux-arm64|windows|windows-native|macos-x86|macos-arm64|gui|all]
param([string]$Target = "all")

$ErrorActionPreference = "Stop"
$BIN          = "sipress"
$OUT          = "dist"
$CARGO        = "cli\Cargo.toml"
$GUI_DIR      = "gui"
$GUI_BIN      = "sipress-gui"
$GUI_RELEASE  = "$GUI_DIR\src-tauri\target\release"
$GUI_BUNDLE   = "$GUI_RELEASE\bundle"

# ── Output helpers ─────────────────────────────────────────────────
function Info ([string]$msg) { Write-Host "[sipress] $msg" -ForegroundColor Cyan  }
function Ok   ([string]$msg) { Write-Host "[  OK  ] $msg"  -ForegroundColor Green }
function Warn ([string]$msg) { Write-Host "[ WARN ] $msg"  -ForegroundColor Yellow }
function Err  ([string]$msg) { Write-Host "[ERROR ] $msg"  -ForegroundColor Red; exit 1 }

# ── Tool checks ────────────────────────────────────────────────────
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

function Ensure-Node {
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Err "node not found. Install from https://nodejs.org" }
    if (-not (Get-Command npm  -ErrorAction SilentlyContinue)) { Err "npm not found. Install from https://nodejs.org"  }
    Info "node $(node --version)  npm $(npm --version)"
}

function Add-RustTarget ([string]$t) {
    $old = $ErrorActionPreference; $ErrorActionPreference = "Continue"
    rustup target add $t | Out-Null
    $ErrorActionPreference = $old
}

# ── CLI build helpers ──────────────────────────────────────────────
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
    Info "Native Windows CLI build ..."
    $null = New-Item -ItemType Directory -Path $OUT -Force
    cargo build --manifest-path $CARGO --release -q
    if ($LASTEXITCODE -ne 0) { Err "Native Windows build failed" }
    $src = "target\release\$BIN.exe"
    $dst = "$OUT\$BIN-windows-x86_64-native.exe"
    Copy-Item -Path $src -Destination $dst -Force
    $mb = [math]::Round((Get-Item $dst).Length / 1MB, 2)
    Ok "$dst  ($mb MB)"
}

# ── GUI helpers ────────────────────────────────────────────────────
function Copy-ToDist ([string]$src, [string]$dstName) {
    if (Test-Path $src) {
        Copy-Item -Path $src -Destination "$OUT\$dstName" -Force
        $mb = [math]::Round((Get-Item "$OUT\$dstName").Length / 1MB, 2)
        Ok "$OUT\$dstName  ($mb MB)"
    } else {
        Warn "Not found (skipped): $src"
    }
}

function Build-Gui {
    Ensure-Node
    $null = New-Item -ItemType Directory -Path $OUT -Force

    Info "npm install ..."
    npm install --prefix $GUI_DIR --silent
    if ($LASTEXITCODE -ne 0) { Err "npm install failed" }

    Info "Tauri build (may take a few minutes) ..."
    npm run --prefix $GUI_DIR tauri build
    if ($LASTEXITCODE -ne 0) { Err "Tauri GUI build failed" }

    # ── Windows: MSI installer + NSIS setup + portable exe ──
    $msi  = Get-ChildItem "$GUI_BUNDLE\msi"  -Filter "*.msi" -ErrorAction SilentlyContinue | Select-Object -First 1
    $nsis = Get-ChildItem "$GUI_BUNDLE\nsis" -Filter "*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    $portable = "$GUI_RELEASE\$GUI_BIN.exe"

    if ($msi)  { Copy-ToDist $msi.FullName  "${GUI_BIN}-windows-x86_64-installer.msi" }
    if ($nsis) { Copy-ToDist $nsis.FullName "${GUI_BIN}-windows-x86_64-setup.exe" }
    Copy-ToDist $portable                   "${GUI_BIN}-windows-x86_64-portable.exe"
}

# ── Main ───────────────────────────────────────────────────────────
Info "Target: $Target"

switch ($Target.ToLower()) {
    "linux-x86" {
        Ensure-Zig; Ensure-Zigbuild
        Add-RustTarget "x86_64-unknown-linux-musl"
        Build-CrossTarget "x86_64-unknown-linux-musl" "$BIN-linux-x86_64"
    }
    "linux-arm64" {
        Ensure-Zig; Ensure-Zigbuild
        Add-RustTarget "aarch64-unknown-linux-musl"
        Build-CrossTarget "aarch64-unknown-linux-musl" "$BIN-linux-arm64"
    }
    "windows" {
        Ensure-Zig; Ensure-Zigbuild
        Add-RustTarget "x86_64-pc-windows-gnu"
        Build-CrossTarget "x86_64-pc-windows-gnu" "$BIN-windows-x86_64.exe"
    }
    "windows-native" {
        Build-NativeWindows
    }
    "macos-x86" {
        Ensure-Zig; Ensure-Zigbuild
        Add-RustTarget "x86_64-apple-darwin"
        Build-CrossTarget "x86_64-apple-darwin" "$BIN-macos-x86_64"
    }
    "macos-arm64" {
        Ensure-Zig; Ensure-Zigbuild
        Add-RustTarget "aarch64-apple-darwin"
        Build-CrossTarget "aarch64-apple-darwin" "$BIN-macos-arm64"
    }
    "gui" {
        Build-Gui
    }
    "all" {
        Ensure-Zig; Ensure-Zigbuild
        Add-RustTarget "x86_64-unknown-linux-musl"
        Add-RustTarget "aarch64-unknown-linux-musl"
        Add-RustTarget "x86_64-pc-windows-gnu"
        Build-CrossTarget "x86_64-unknown-linux-musl"  "$BIN-linux-x86_64"
        Build-CrossTarget "aarch64-unknown-linux-musl" "$BIN-linux-arm64"
        Build-CrossTarget "x86_64-pc-windows-gnu"      "$BIN-windows-x86_64.exe"
        Build-NativeWindows
        Build-Gui
    }
    default {
        Write-Host "Usage: .\build.ps1 [TARGET]"
        Write-Host ""
        Write-Host "  CLI:"
        Write-Host "    linux-x86        Linux x86_64 static (musl)"
        Write-Host "    linux-arm64      Linux ARM64  static (musl)"
        Write-Host "    windows          Windows x86_64 GNU (zigbuild)"
        Write-Host "    windows-native   Windows x86_64 MSVC (host)"
        Write-Host "    macos-x86        macOS x86_64"
        Write-Host "    macos-arm64      macOS ARM64"
        Write-Host ""
        Write-Host "  GUI:"
        Write-Host "    gui              Tauri GUI — installer (.msi/.exe) + portable (.exe)"
        Write-Host ""
        Write-Host "  Combined:"
        Write-Host "    all              all CLI targets + GUI  [default]"
        exit 1
    }
}

Info "Done. Output: $OUT\"
if (Test-Path $OUT) {
    Get-ChildItem $OUT | Sort-Object Name | ForEach-Object {
        $mb = [math]::Round($_.Length / 1MB, 2)
        Write-Host ("  {0,-56} {1,6} MB" -f $_.Name, $mb)
    }
}
