#!/usr/bin/env bash
# sipress cross-platform build script (CLI + GUI)
set -euo pipefail

PROFILE="release"
OUT="dist"
BIN="sipress"
GUI_DIR="gui"

# ── Colored output ────────────────────────────────────────────────
info() { echo -e "\033[36m[sipress]\033[0m $*"; }
ok()   { echo -e "\033[32m[  OK  ]\033[0m $*"; }
err()  { echo -e "\033[31m[ERROR ]\033[0m $*" >&2; exit 1; }

# ── Auto-detect zig ───────────────────────────────────────────────
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

ensure_node() {
    command -v node &>/dev/null || err "node not found. Install from https://nodejs.org"
    command -v npm  &>/dev/null || err "npm not found. Install from https://nodejs.org"
    info "node $(node --version)  npm $(npm --version)"
}

mkdir -p "$OUT"

# ── Cross-compile CLI via cargo-zigbuild ──────────────────────────
build_cross() {
    local target="$1" out_name="$2"
    info "Cross-compiling $target ..."
    cargo zigbuild \
        --manifest-path cli/Cargo.toml \
        --target "$target" \
        --$PROFILE \
        -q
    local src="target/$target/$PROFILE/$BIN"
    [[ "$out_name" == *.exe ]] && src="${src}.exe"
    cp "$src" "$OUT/$out_name"
    local size; size=$(du -sh "$OUT/$out_name" | cut -f1)
    ok "$OUT/$out_name  ($size)"
}

# ── Native CLI build (host toolchain) ────────────────────────────
build_native() {
    info "Native build (host toolchain) ..."
    cargo build \
        --manifest-path cli/Cargo.toml \
        --$PROFILE \
        -q
    local src="target/$PROFILE/$BIN"
    local out_name="$BIN-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" || "$OSTYPE" == "win32" ]]; then
        src="${src}.exe"
        out_name="${BIN}-windows-x86_64-native.exe"
    fi
    cp "$src" "$OUT/$out_name"
    local size; size=$(du -sh "$OUT/$out_name" | cut -f1)
    ok "$OUT/$out_name  ($size)"
}

# ── GUI build (Tauri) ────────────────────────────────────────────
build_gui() {
    ensure_node
    info "Installing npm dependencies ..."
    npm install --prefix "$GUI_DIR" --silent

    info "Building Tauri GUI (this may take a few minutes) ..."
    npm run --prefix "$GUI_DIR" tauri build

    # Report bundle output location
    local bundle_dir="$GUI_DIR/src-tauri/target/release/bundle"
    if [[ -d "$bundle_dir" ]]; then
        ok "GUI bundles → $bundle_dir/"
        find "$bundle_dir" -maxdepth 2 \( -name "*.msi" -o -name "*.exe" -o -name "*.deb" \
            -o -name "*.AppImage" -o -name "*.dmg" -o -name "*.rpm" \) \
            -exec sh -c 'size=$(du -sh "$1" | cut -f1); echo "  [  OK  ] $1  ($size)"' _ {} \;
    fi
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
    gui)
        build_gui
        ;;
    all)
        ensure_zig; ensure_zigbuild
        rustup target add x86_64-unknown-linux-musl  2>/dev/null || true
        rustup target add aarch64-unknown-linux-musl 2>/dev/null || true
        rustup target add x86_64-pc-windows-gnu      2>/dev/null || true
        build_cross "x86_64-unknown-linux-musl"  "sipress-linux-x86_64"
        build_cross "aarch64-unknown-linux-musl" "sipress-linux-arm64"
        build_cross "x86_64-pc-windows-gnu"      "sipress-windows-x86_64.exe"
        build_native
        build_gui
        ;;
    *)
        echo "Usage: $0 [TARGET]"
        echo ""
        echo "  CLI targets:"
        echo "    linux-x86        Linux x86_64 static binary (musl)"
        echo "    linux-arm64      Linux ARM64  static binary (musl)"
        echo "    windows          Windows x86_64 (GNU, via zigbuild)"
        echo "    windows-native   Native host build (no zigbuild required)"
        echo "    macos-x86        macOS x86_64 (requires macOS SDK)"
        echo "    macos-arm64      macOS ARM64  (requires macOS SDK)"
        echo ""
        echo "  GUI targets:"
        echo "    gui              Tauri GUI app for host platform (requires node/npm)"
        echo ""
        echo "  Combined:"
        echo "    all              linux-x86 + linux-arm64 + windows + native + gui"
        exit 1
        ;;
esac

info "Done."
if [[ "$TARGET" != "gui" ]]; then
    info "CLI output: $OUT/"
    ls -lh "$OUT"/ 2>/dev/null || true
fi
