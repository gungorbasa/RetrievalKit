#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
KOTLIN_DIR=$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)
REPO_DIR=$(CDPATH='' cd -- "$KOTLIN_DIR/../.." && pwd)
MANIFEST="$REPO_DIR/crates/retrievalkit-jni-embedding/Cargo.toml"
MODE=${1:-all}

case "$MODE" in
  jvm|android|all) ;;
  *)
    echo "usage: $0 [jvm|android|all]" >&2
    exit 2
    ;;
esac

"$SCRIPT_DIR/preflight.sh" "$MODE"

require_file() {
  if [ ! -f "$1" ]; then
    echo "Kotlin embedding native build failed: required file is missing: $1" >&2
    exit 1
  fi
}

install_runtime_tree() {
  source=$1
  destination=$2
  mkdir -p "$destination"
  for name in \
    ONNX-Runtime-LICENSE \
    ONNX-Runtime-ThirdPartyNotices.txt \
    runtime-identity.txt
  do
    install -m 0644 "$source/$name" "$destination/$name"
  done
}

install_project_legal_tree() {
  destination=$1
  mkdir -p "$destination"
  install -m 0644 "$REPO_DIR/LICENSE" "$destination/LICENSE"
  install -m 0644 "$REPO_DIR/NOTICE" "$destination/NOTICE"
}

prepare_macos_runtime() {
  : "${RETRIEVALKIT_ONNX_RUNTIME_LIBRARY:?Set RETRIEVALKIT_ONNX_RUNTIME_LIBRARY to the qualified libonnxruntime.1.24.3.dylib}"
  runtime_dir=$(dirname -- "$RETRIEVALKIT_ONNX_RUNTIME_LIBRARY")
  runtime_license=${RETRIEVALKIT_ONNX_RUNTIME_LICENSE:-"$runtime_dir/LICENSE"}
  runtime_notices=${RETRIEVALKIT_ONNX_RUNTIME_NOTICES:-"$runtime_dir/ThirdPartyNotices.txt"}
  prepared="$REPO_DIR/target/kotlin-embedding-runtime/macos-aarch64"
  python3 "$SCRIPT_DIR/prepare-embedding-runtime.py" macos \
    --runtime "$RETRIEVALKIT_ONNX_RUNTIME_LIBRARY" \
    --license "$runtime_license" \
    --notices "$runtime_notices" \
    --output "$prepared"
  printf '%s\n' "$prepared"
}

prepare_android_runtime() {
  : "${RETRIEVALKIT_ONNX_RUNTIME_ANDROID_AAR:?Set RETRIEVALKIT_ONNX_RUNTIME_ANDROID_AAR to the pinned onnxruntime-android-1.24.3.aar}"
  : "${RETRIEVALKIT_ONNX_RUNTIME_LICENSE:?Set RETRIEVALKIT_ONNX_RUNTIME_LICENSE to the official ONNX Runtime 1.24.3 LICENSE}"
  : "${RETRIEVALKIT_ONNX_RUNTIME_NOTICES:?Set RETRIEVALKIT_ONNX_RUNTIME_NOTICES to the official ONNX Runtime 1.24.3 ThirdPartyNotices.txt}"
  prepared="$REPO_DIR/target/kotlin-embedding-runtime/android-arm64-v8a"
  python3 "$SCRIPT_DIR/prepare-embedding-runtime.py" android \
    --aar "$RETRIEVALKIT_ONNX_RUNTIME_ANDROID_AAR" \
    --license "$RETRIEVALKIT_ONNX_RUNTIME_LICENSE" \
    --notices "$RETRIEVALKIT_ONNX_RUNTIME_NOTICES" \
    --output "$prepared"
  printf '%s\n' "$prepared"
}

build_jvm() {
  prepared=$(prepare_macos_runtime)
  cargo build --locked --manifest-path "$MANIFEST" --release
  native="$REPO_DIR/target/release/libretrievalkit_embedding_jni.dylib"
  require_file "$native"

  resources="$KOTLIN_DIR/embedding/build/generated/resources"
  platform="$resources/native/macos-aarch64"
  mkdir -p "$platform"
  install -m 0755 "$native" "$platform/libretrievalkit_embedding_jni.dylib"
  install -m 0755 \
    "$prepared/libonnxruntime.1.24.3.dylib" \
    "$platform/libonnxruntime.1.24.3.dylib"
  install_runtime_tree "$prepared" "$resources"
}

build_android() {
  prepared=$(prepare_android_runtime)
  : "${ANDROID_NDK_HOME:=$HOME/Library/Android/sdk/ndk/26.1.10909125}"
  toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
  linker="$toolchain/aarch64-linux-android24-clang"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$linker"
  export CC_aarch64_linux_android="$linker"
  export AR_aarch64_linux_android="$toolchain/llvm-ar"

  cargo build --locked --manifest-path "$MANIFEST" \
    --target aarch64-linux-android --release
  native="$REPO_DIR/target/aarch64-linux-android/release/libretrievalkit_embedding_jni.so"
  require_file "$native"

  generated="$KOTLIN_DIR/android-embedding/build/generated"
  platform="$generated/jniLibs/arm64-v8a"
  mkdir -p "$platform"
  install -m 0755 "$native" "$platform/libretrievalkit_embedding_jni.so"
  "$toolchain/llvm-strip" --strip-unneeded \
    "$platform/libretrievalkit_embedding_jni.so"
  install -m 0755 "$prepared/libonnxruntime.so" "$platform/libonnxruntime.so"
  install_project_legal_tree "$generated/resources"
  install_runtime_tree "$prepared" "$generated/resources"
}

case "$MODE" in
  jvm) build_jvm ;;
  android) build_android ;;
  all)
    build_jvm
    build_android
    ;;
esac
