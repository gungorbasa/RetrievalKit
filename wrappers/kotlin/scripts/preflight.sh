#!/bin/sh
set -eu

MODE=${1:-all}

fail() {
  echo "Kotlin wrapper preflight failed: $*" >&2
  exit 1
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "required tool '$1' was not found on PATH."
  fi
}

case "$MODE" in
  jvm|android|all) ;;
  *)
    echo "usage: $0 [jvm|android|all]" >&2
    exit 2
    ;;
esac

require_tool java
require_tool cargo

JAVA_VERSION_OUTPUT=$(java -version 2>&1) ||
  fail "could not run 'java -version'."
JAVA_VERSION=$(printf '%s\n' "$JAVA_VERSION_OUTPUT" | sed -n '1s/.*version "\([^"]*\)".*/\1/p')
JAVA_MAJOR=$(printf '%s\n' "$JAVA_VERSION" | sed 's/^\([0-9][0-9]*\).*/\1/')
if [ -z "$JAVA_VERSION" ] || [ "$JAVA_MAJOR" != "17" ]; then
  fail "JDK 17 is required; detected Java ${JAVA_VERSION:-unknown}. Set JAVA_HOME to a JDK 17 installation and put its bin directory first on PATH."
fi

HOST_OS=$(uname -s)
HOST_ARCH=$(uname -m)
if [ "$HOST_OS" != "Darwin" ] || [ "$HOST_ARCH" != "arm64" ]; then
  fail "the initial Kotlin/JVM native package requires macOS arm64; detected $HOST_OS $HOST_ARCH."
fi

CARGO_VERSION=$(cargo --version)

echo "Kotlin wrapper preflight passed"
echo "  Java: required JDK 17; detected $JAVA_VERSION"
echo "  Rust: required cargo on PATH; detected $CARGO_VERSION"
echo "  JVM host: required Darwin arm64; detected $HOST_OS $HOST_ARCH"

if [ "$MODE" = "android" ] || [ "$MODE" = "all" ]; then
  require_tool rustup
  if ! rustup target list --installed | grep -qx 'aarch64-linux-android'; then
    fail "Rust target aarch64-linux-android is required; run 'rustup target add aarch64-linux-android'."
  fi

  : "${ANDROID_NDK_HOME:=$HOME/Library/Android/sdk/ndk/26.1.10909125}"
  TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
  LINKER="$TOOLCHAIN/aarch64-linux-android24-clang"
  if [ ! -x "$LINKER" ]; then
    fail "Android NDK 26 arm64 linker not found at $LINKER. Set ANDROID_NDK_HOME to the installed NDK 26 directory."
  fi

  echo "  Android: required API 24 arm64-v8a with NDK 26; detected toolchain at $ANDROID_NDK_HOME"
fi
