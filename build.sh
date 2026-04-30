#!/usr/bin/env bash
# sipress 靜態交叉編譯腳本（cargo-zigbuild）
set -euo pipefail

PROFILE="release"
OUT="dist"
BIN="sipress"

# 顏色輸出
info()  { echo -e "\033[36m[sipress]\033[0m $*"; }
ok()    { echo -e "\033[32m[  OK  ]\033[0m $*"; }
err()   { echo -e "\033[31m[ERROR ]\033[0m $*" >&2; exit 1; }

# 確認工具存在
command -v cargo-zigbuild &>/dev/null || err "請先安裝 cargo-zigbuild: cargo install cargo-zigbuild"
command -v zig             &>/dev/null || err "請先安裝 zig: pip install ziglang 或從 ziglang.org 下載"

mkdir -p "$OUT"

build_target() {
    local target="$1"
    local out_name="$2"
    info "編譯 $target ..."
    cargo zigbuild \
        --manifest-path cli/Cargo.toml \
        --target "$target" \
        --$PROFILE \
        -q
    local src="target/$target/$PROFILE/$BIN"
    [[ "$out_name" == *.exe ]] || true
    cp "$src" "$OUT/$out_name"
    ok "$OUT/$out_name"
}

TARGET="${1:-all}"

case "$TARGET" in
    linux-x86)
        build_target "x86_64-unknown-linux-musl" "sipress-linux-x86_64"
        ;;
    linux-arm64)
        build_target "aarch64-unknown-linux-musl" "sipress-linux-arm64"
        ;;
    windows)
        build_target "x86_64-pc-windows-gnu" "sipress-windows-x86_64.exe"
        ;;
    macos-x86)
        build_target "x86_64-apple-darwin" "sipress-macos-x86_64"
        ;;
    macos-arm64)
        build_target "aarch64-apple-darwin" "sipress-macos-arm64"
        ;;
    all)
        build_target "x86_64-unknown-linux-musl"  "sipress-linux-x86_64"
        build_target "aarch64-unknown-linux-musl"  "sipress-linux-arm64"
        build_target "x86_64-pc-windows-gnu"       "sipress-windows-x86_64.exe"
        ;;
    *)
        echo "用法: $0 [linux-x86|linux-arm64|windows|macos-x86|macos-arm64|all]"
        exit 1
        ;;
esac

info "輸出目錄: $OUT/"
ls -lh "$OUT"/
