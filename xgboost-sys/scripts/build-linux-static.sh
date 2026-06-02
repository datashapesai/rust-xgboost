#!/usr/bin/env bash
# build-linux-static.sh
#
# Builds libxgboost.a (static) for Linux x86_64 and Linux arm64 inside
# Docker containers, then strips debug symbols and copies the archives into
# the correct prebuilt-lib directories so build.rs can find them.
#
# Prerequisites:
#   - Docker with buildx support (standard on Docker Desktop ≥ 4.x)
#   - Run from the repository root:
#       xgboost-sys/scripts/build-linux-static.sh
#
# ── Alternative: Cargo-driven build ─────────────────────────────────────────
# If you have a Rust toolchain, clang/libclang (for bindgen), CMake, and
# Ninja available in your Linux environment, you can skip this script and
# drive the build through Cargo directly:
#
#   cargo build \
#     --manifest-path xgboost-sys/Cargo.toml \
#     --no-default-features \
#     --features local_build,static_link
#
# The `static_link` feature sets BUILD_STATIC_LIB=ON in the CMake invocation
# and ensures the final link directive uses `static=xgboost`.  After the
# build, find the produced archives under target/ and copy them to
# xgboost-sys/lib/linux_{amd64,arm64}/ before committing.
#
# This script uses raw CMake (no Rust toolchain needed) to keep the Docker
# image minimal and the build fast.
# ────────────────────────────────────────────────────────────────────────────
#
# Output:
#   xgboost-sys/lib/linux_amd64/libxgboost.a   (~20 MB after strip)
#   xgboost-sys/lib/linux_amd64/libdmlc.a
#   xgboost-sys/lib/linux_arm64/libxgboost.a   (~20 MB after strip)
#   xgboost-sys/lib/linux_arm64/libdmlc.a
#
# The existing .so files are left in place — they are not removed.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
XGB_SYS="$REPO_ROOT/xgboost-sys"
XGB_SRC="$XGB_SYS/xgboost"

if [[ ! -f "$XGB_SRC/CMakeLists.txt" ]]; then
    echo "ERROR: XGBoost submodule not found at $XGB_SRC" >&2
    echo "       Run: git submodule update --init --recursive" >&2
    exit 1
fi

# Inline Dockerfile used for both architectures.
# Debian 12 (bookworm) ships CMake 3.25+ and gcc-12, both sufficient for XGBoost 3.x.
read -r -d '' DOCKERFILE <<'DOCKERFILE_EOF' || true
FROM debian:12-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake \
        ninja-build \
        g++ \
        gcc \
        make \
        ca-certificates \
        llvm \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
DOCKERFILE_EOF

build_for_platform() {
    local platform="$1"    # e.g. linux/amd64 or linux/arm64
    local out_dir="$2"     # e.g. linux_amd64 or linux_arm64
    local tag="xgb-static-build-${out_dir}"

    echo ""
    echo "========================================================"
    echo " Building libxgboost.a for $platform  →  lib/$out_dir/"
    echo "========================================================"

    # Build the builder image for the target platform.
    docker buildx build \
        --platform "$platform" \
        --load \
        -t "$tag" \
        - <<< "$DOCKERFILE"

    # Mount source read-write: CMake's write_version step writes version_config.h
    # into the source tree (an XGBoost CMake quirk).  We restore the submodule
    # to a clean state after the container exits.
    docker run --rm \
        --platform "$platform" \
        -v "$XGB_SRC":/src/xgboost \
        -v "xgb-out-${out_dir}":/out \
        "$tag" \
        bash -c "
            set -euxo pipefail
            cmake \
                -S /src/xgboost \
                -B /build \
                -G Ninja \
                -DCMAKE_BUILD_TYPE=Release \
                -DBUILD_STATIC_LIB=ON \
                -DBUILD_SHARED_LIBS=OFF \
                -DUSE_OPENMP=ON \
                -DUSE_CUDA=OFF \
                -DUSE_NCCL=OFF
            ninja -C /build xgboost dmlc
            # XGBoost writes libxgboost.a into the source tree's lib/ dir (CMake quirk).
            # libdmlc.a goes into the build dir's dmlc-core/ subdir.
            cp /src/xgboost/lib/libxgboost.a /out/libxgboost.a
            cp /build/dmlc-core/libdmlc.a    /out/libdmlc.a
            # Strip debug symbols — keeps the archive under GitHub's 100 MB limit.
            llvm-strip --strip-debug /out/libxgboost.a
            llvm-strip --strip-debug /out/libdmlc.a
            echo 'Build complete.'
        "

    # Restore any files CMake wrote into the submodule source tree.
    git -C "$XGB_SRC" checkout -- . 2>/dev/null || true
    git -C "$XGB_SRC" clean -fd lib/ 2>/dev/null || true

    # Extract artifacts from the named volume via a temporary container.
    local dest="$XGB_SYS/lib/$out_dir"
    docker run --rm \
        --platform "$platform" \
        -v "xgb-out-${out_dir}":/out:ro \
        -v "$dest":/dest \
        debian:12-slim \
        cp /out/libxgboost.a /out/libdmlc.a /dest/

    docker volume rm "xgb-out-${out_dir}" >/dev/null

    echo "Artifacts written to $dest/"
    ls -lh "$dest/libxgboost.a" "$dest/libdmlc.a"
}

build_for_platform "linux/amd64" "linux_amd64"
build_for_platform "linux/arm64" "linux_arm64"

echo ""
echo "All builds complete. Review the files, then commit:"
echo "  git add xgboost-sys/lib/linux_amd64/libxgboost.a"
echo "  git add xgboost-sys/lib/linux_amd64/libdmlc.a"
echo "  git add xgboost-sys/lib/linux_arm64/libxgboost.a"
echo "  git add xgboost-sys/lib/linux_arm64/libdmlc.a"

