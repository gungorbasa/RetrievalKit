#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
KOTLIN_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
REPO_DIR=$(CDPATH= cd -- "$KOTLIN_DIR/../.." && pwd)
MANIFEST="$REPO_DIR/crates/retrievalkit-jni/Cargo.toml"
MODE=${1:-all}

"$SCRIPT_DIR/preflight.sh" "$MODE"

install_licenses() {
  destination=$1
  mkdir -p "$destination"
  install -m 0644 "$REPO_DIR/LICENSE" "$destination/LICENSE"
  install -m 0644 "$REPO_DIR/NOTICE" "$destination/NOTICE"
}

build_jvm() {
  cargo build --manifest-path "$MANIFEST" --release
  base_resources="$KOTLIN_DIR/base/build/generated/resources"
  install_licenses "$base_resources"
  mkdir -p "$base_resources/native/macos-aarch64"
  install -m 0755 \
    "$REPO_DIR/target/release/libretrievalkit_jni.dylib" \
    "$base_resources/native/macos-aarch64/libretrievalkit_jni.dylib"

  cargo build --manifest-path "$MANIFEST" --release --features graph
  graph_resources="$KOTLIN_DIR/graph/build/generated/resources"
  install_licenses "$graph_resources"
  mkdir -p "$graph_resources/native/macos-aarch64"
  install -m 0755 \
    "$REPO_DIR/target/release/libretrievalkit_jni.dylib" \
    "$graph_resources/native/macos-aarch64/libretrievalkit_jni_graph.dylib"
}

build_android() {
  : "${ANDROID_NDK_HOME:=$HOME/Library/Android/sdk/ndk/26.1.10909125}"
  toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
  linker="$toolchain/aarch64-linux-android24-clang"
  if [ ! -x "$linker" ]; then
    echo "Android NDK linker not found at $linker" >&2
    echo "Set ANDROID_NDK_HOME to an NDK containing the LLVM arm64 Android toolchain." >&2
    exit 1
  fi
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$linker"
  export CC_aarch64_linux_android="$linker"
  export AR_aarch64_linux_android="$toolchain/llvm-ar"

  cargo build --manifest-path "$MANIFEST" --target aarch64-linux-android --release
  base_android="$KOTLIN_DIR/android-base/build/generated"
  install_licenses "$base_android/resources"
  mkdir -p "$base_android/jniLibs/arm64-v8a"
  install -m 0755 \
    "$REPO_DIR/target/aarch64-linux-android/release/libretrievalkit_jni.so" \
    "$base_android/jniLibs/arm64-v8a/libretrievalkit_jni.so"
  "$toolchain/llvm-strip" --strip-unneeded \
    "$base_android/jniLibs/arm64-v8a/libretrievalkit_jni.so"

  cargo build --manifest-path "$MANIFEST" --target aarch64-linux-android --release --features graph
  graph_android="$KOTLIN_DIR/android-graph/build/generated"
  install_licenses "$graph_android/resources"
  mkdir -p "$graph_android/jniLibs/arm64-v8a"
  install -m 0755 \
    "$REPO_DIR/target/aarch64-linux-android/release/libretrievalkit_jni.so" \
    "$graph_android/jniLibs/arm64-v8a/libretrievalkit_jni_graph.so"
  "$toolchain/llvm-strip" --strip-unneeded \
    "$graph_android/jniLibs/arm64-v8a/libretrievalkit_jni_graph.so"
}

case "$MODE" in
  jvm) build_jvm ;;
  android) build_android ;;
  all)
    build_jvm
    build_android
    ;;
  *)
    echo "usage: $0 [jvm|android|all]" >&2
    exit 2
    ;;
esac
