#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HEADER_PATH="$ROOT_DIR/crates/vectorkit-ffi/include/vectorkit_ffi.h"
GRAPH_HEADER_PATH="$ROOT_DIR/crates/vectorkit-ffi/include/vectorkit_graph_ffi.h"
BUILD_DIR="$ROOT_DIR/target/apple"
FRAMEWORK_NAME="VectorKitFFI"
GRAPH_BUILD=0
MIN_IOS_VERSION="${MIN_IOS_VERSION:-15.0}"
MIN_MACOS_VERSION="${MIN_MACOS_VERSION:-14.0}"

MACOS_TARGET="aarch64-apple-darwin"
IOS_DEVICE_TARGET="aarch64-apple-ios"
IOS_SIMULATOR_TARGET="aarch64-apple-ios-sim"

usage() {
  cat <<'EOF'
usage:
  scripts/build-xcframework.sh [--macos-only] [--graph]

Builds target/apple/VectorKitFFI.xcframework from vectorkit-ffi.

Options:
  --macos-only   build only the local macOS arm64 slice; useful for script smoke checks
  --graph        build the aggregate VectorKitGraphFFI artifact instead of the base artifact
  --help, -h     show this help

Install all Apple Rust targets before the full build:
  rustup target add aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim
EOF
}

MACOS_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --macos-only)
      MACOS_ONLY=1
      ;;
    --graph)
      GRAPH_BUILD=1
      FRAMEWORK_NAME="VectorKitGraphFFI"
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

XCFRAMEWORK_PATH="$BUILD_DIR/$FRAMEWORK_NAME.xcframework"

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "required tool not found: $1" >&2
    exit 1
  fi
}

rust_target_installed() {
  rustup target list --installed | grep -qx "$1"
}

build_with_deployment_target() {
  local rust_target="$1"
  local platform="$2"
  local min_version="$3"

  case "$platform" in
    macos)
      MACOSX_DEPLOYMENT_TARGET="$min_version" \
        cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p vectorkit-ffi --release --target "$rust_target" ${CARGO_FEATURE_ARGS[@]+"${CARGO_FEATURE_ARGS[@]}"}
      ;;
    ios)
      IPHONEOS_DEPLOYMENT_TARGET="$min_version" \
        cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p vectorkit-ffi --release --target "$rust_target" ${CARGO_FEATURE_ARGS[@]+"${CARGO_FEATURE_ARGS[@]}"}
      ;;
    ios-simulator)
      IPHONEOS_DEPLOYMENT_TARGET="$min_version" \
      IPHONESIMULATOR_DEPLOYMENT_TARGET="$min_version" \
        cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p vectorkit-ffi --release --target "$rust_target" ${CARGO_FEATURE_ARGS[@]+"${CARGO_FEATURE_ARGS[@]}"}
      ;;
    *)
      echo "unsupported platform for $rust_target: $platform" >&2
      exit 1
      ;;
  esac
}

framework_info_plist() {
  local framework_dir="$1"
  cat >"$framework_dir/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>$FRAMEWORK_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>dev.vectorkit.$FRAMEWORK_NAME</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$FRAMEWORK_NAME</string>
  <key>CFBundlePackageType</key>
  <string>FMWK</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
</dict>
</plist>
EOF
}

module_map() {
  local modules_dir="$1"
  cat >"$modules_dir/module.modulemap" <<EOF
framework module $FRAMEWORK_NAME {
  umbrella header "$FRAMEWORK_NAME.h"
  export *
  module * { export * }
}
EOF
}

create_framework_from_static_lib() {
  local static_lib="$1"
  local slice_name="$2"
  local framework_dir="$BUILD_DIR/slices/$slice_name/$FRAMEWORK_NAME.framework"
  local headers_dir="$framework_dir/Headers"
  local modules_dir="$framework_dir/Modules"

  rm -rf "$framework_dir"
  mkdir -p "$headers_dir" "$modules_dir"
  cp "$static_lib" "$framework_dir/$FRAMEWORK_NAME"
  if [[ "$GRAPH_BUILD" == "1" ]]; then
    cp "$HEADER_PATH" "$headers_dir/vectorkit_ffi.h"
    cp "$GRAPH_HEADER_PATH" "$headers_dir/$FRAMEWORK_NAME.h"
  else
    cp "$HEADER_PATH" "$headers_dir/$FRAMEWORK_NAME.h"
  fi
  module_map "$modules_dir"
  framework_info_plist "$framework_dir"

  printf '%s\n' "$framework_dir"
}

build_rust_target() {
  local rust_target="$1"
  local platform="$2"
  local min_version="$3"

  echo "Building $rust_target" >&2
  build_with_deployment_target "$rust_target" "$platform" "$min_version"
}

build_framework_slice() {
  local rust_target="$1"
  local platform="$2"
  local slice_name="$3"
  local min_version="$4"

  build_rust_target "$rust_target" "$platform" "$min_version"
  create_framework_from_static_lib \
    "$ROOT_DIR/target/$rust_target/release/libvectorkit_ffi.a" \
    "$slice_name"
}

main() {
  require_tool cargo
  require_tool rustup
  require_tool xcodebuild

  if [[ ! -f "$HEADER_PATH" ]]; then
    echo "missing header: $HEADER_PATH" >&2
    exit 1
  fi
  if [[ "$GRAPH_BUILD" == "1" && ! -f "$GRAPH_HEADER_PATH" ]]; then
    echo "missing graph header: $GRAPH_HEADER_PATH" >&2
    exit 1
  fi

  CARGO_FEATURE_ARGS=()
  if [[ "$GRAPH_BUILD" == "1" ]]; then
    CARGO_FEATURE_ARGS=(--features graph)
  fi

  local required_targets=("$MACOS_TARGET" "$IOS_DEVICE_TARGET" "$IOS_SIMULATOR_TARGET")
  if [[ "$MACOS_ONLY" == "1" ]]; then
    required_targets=("$MACOS_TARGET")
  fi

  local missing_targets=()
  for rust_target in "${required_targets[@]}"; do
    if ! rust_target_installed "$rust_target"; then
      missing_targets+=("$rust_target")
    fi
  done

  if [[ "${#missing_targets[@]}" -gt 0 ]]; then
    echo "missing Rust target(s): ${missing_targets[*]}" >&2
    echo "install with:" >&2
    echo "  rustup target add ${missing_targets[*]}" >&2
    exit 1
  fi

  rm -rf "$BUILD_DIR/slices" "$XCFRAMEWORK_PATH"
  mkdir -p "$BUILD_DIR/slices"

  local framework_args=()
  local framework_dir

  framework_dir="$(build_framework_slice "$MACOS_TARGET" "macos" "macos-arm64" "$MIN_MACOS_VERSION")"
  framework_args+=("-framework" "$framework_dir")

  if [[ "$MACOS_ONLY" != "1" ]]; then
    framework_dir="$(build_framework_slice "$IOS_DEVICE_TARGET" "ios" "ios-arm64" "$MIN_IOS_VERSION")"
    framework_args+=("-framework" "$framework_dir")

    framework_dir="$(build_framework_slice "$IOS_SIMULATOR_TARGET" "ios-simulator" "ios-simulator-arm64" "$MIN_IOS_VERSION")"
    framework_args+=("-framework" "$framework_dir")
  fi

  echo "Creating $XCFRAMEWORK_PATH"
  xcodebuild -create-xcframework "${framework_args[@]}" -output "$XCFRAMEWORK_PATH"
  echo "Created $XCFRAMEWORK_PATH"
}

main "$@"
