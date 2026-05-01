#!/usr/bin/env bash
# sipress cross-platform static build script (cargo-zigbuild)
set -euo pipefail

PROFILE="release"
OUT="dist"
BIN="sipress"

# ── Colored output ────────────────────────────────────────────────
info() { echo -e "\033[36m[sipress]\033[0m $*"; }
ok()   { echo -e "\033[32m[  OK  ]\033[0m $*"; }
err()  { echo -e "\033[31m[ERROR ]\033[0m $*" >&2; exit 1; }

# ── Auto-detect zig ───────────────────────────────────────────────
# First checks PATH; if absent, looks inside the Python ziglang package
ensure_zig() {
    if command -v zig &>/dev/null; then
        info "zig $(zig version) found in PATH"
        return
    fi

    local zig_dir
    zig_dir=$(python -c "import ziglang, os; print(os.path.dirname(ziglang.__file__))" 2>/dev/null || true)

    if [[ -n "$zig_dir" && ( -f "$zig_dir/zig" || -f "$zig_dir/zig.exe" ) ]]; then
        export PATH="$zig_dir:$PATH"
        info "zig $(zig version) (ziglang package: $zig_dir)"
    else
        err "zig not found. Install: pip install ziglang  OR  download from ziglang.org"
    fi
}

ensure_zigbuild() {
    command -v cargo-zigbuild &>/dev/null \
        || err "cargo-zigbuild not found. Install: cargo install cargo-zigbuild"
}

mkdir -p "$OUT"

# ── Cross-compile via cargo-zigbuild ─────────────────────────────
build_cross() {
    local target="$1" out_name="$2"
    info "Cross-compiling $target ..."
    cargo zigbuild \
        --manifest-path cli/Cargo.toml \
        --target "$target" \
        --$PROFILE \
        -q
    local src="target/$target/$PROFILE/$BIN"
    # Append .exe for Windows targets
    [[ "$out_name" == *.exe ]] && src="${src}.exe"
    cp "$src" "$OUT/$out_name"
    local size
    size=$(du -sh "$OUT/$out_name" | cut -f1)
    ok "$OUT/$out_name  ($size)"
}

# ── Native build (no zigbuild, uses host toolchain) ──────────────
build_native() {
    info "Native build (host toolchain, no zigbuild) ..."
    cargo build \
        --manifest-path cli/Cargo.toml \
        --$PROFILE \
        -q

    # Detect output name based on OS
    local src="target/$PROFILE/$BIN"
    local out_name="$BIN-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" || "$OSTYPE" == "win32" ]]; then
        src="${src}.exe"
        out_name="${BIN}-windows-x86_64-native.exe"
    fi
    cp "$src" "$OUT/$out_name"
    local size
    size=$(du -sh "$OUT/$out_name" | cut -f1)
    ok "$OUT/$out_name  ($size)"
}

TARGET="${1:-all}"

case "$TARGET" in
    linux-x86)
        ensure_zig; ensure_zigbuild
        rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
        build_cross "x86_64-unknown-linux-musl" "sipress-linux-x86_64"
        ;;
    linux-arm64)
        ensure_zig; ensure_zigbuild
        rustup target add aarch64-unknown-linux-musl 2>/dev/null || true
        build_cross "aarch64-unknown-linux-musl" "sipress-linux-arm64"
        ;;
    windows)
        ensure_zig; ensure_zigbuild
        rustup target add x86_64-pc-windows-gnu 2>/dev/null || true
        build_cross "x86_64-pc-windows-gnu" "sipress-windows-x86_64.exe"
        ;;
    windows-native)
        build_native
        ;;
    macos-x86)
        ensure_zig; ensure_zigbuild
        rustup target add x86_64-apple-darwin 2>/dev/null || true
        build_cross "x86_64-apple-darwin" "sipress-macos-x86_64"
        ;;
    macos-arm64)
        ensure_zig; ensure_zigbuild
        rustup target add aarch64-apple-darwin 2>/dev/null || true
        build_cross "aarch64-apple-darwin" "sipress-macos-arm64"
        ;;
    all)
        ensure_zig; ensure_zigbuild
        rustup target add x86_64-unknown-linux-musl  -q 2>/dev/null || true
        rustup target add aarch64-unknown-linux-musl 2>/dev/null || true
        rustup target add x86_64-pc-windows-gnu      -q 2>/dev/null || true
        build_cross "x86_64-unknown-linux-musl"  "sipress-linux-x86_64"
        build_cross "aarch64-unknown-linux-musl" "sipress-linux-arm64"
        build_cross "x86_64-pc-windows-gnu"      "sipress-windows-x86_64.exe"
        build_native
        ;;
    *)
        echo "Usage: $0 [linux-x86|linux-arm64|windows|windows-native|macos-x86|macos-arm64|all]"
        echo ""
        echo "  linux-x86        Linux x86_64 static binary (musl)"
        echo "  linux-arm64      Linux ARM64  static binary (musl)"
        echo "  windows          Windows x86_64 (GNU, via zigbuild)"
        echo "  windows-native   Native host build (no zigbuild required)"
        echo "  macos-x86        macOS x86_64 (requires macOS SDK)"
        echo "  macos-arm64      macOS ARM64  (requires macOS SDK)"
        echo "  all              linux-x86 + linux-arm64 + windows + native"
        exit 1
        ;;
esac

info "Output dir: $OUT/"
ls -lh "$OUT"/
