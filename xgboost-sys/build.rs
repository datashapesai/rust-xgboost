use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn emit_version_env(xgb_root: &Path) {
    let version_config = xgb_root.join("include").join("xgboost").join("version_config.h");
    let contents =
        fs::read_to_string(&version_config).unwrap_or_else(|_| panic!("Cannot read {}", version_config.display()));

    for line in contents.lines() {
        for (define, env_key) in &[
            ("XGBOOST_VER_MAJOR", "XGBOOST_VER_MAJOR"),
            ("XGBOOST_VER_MINOR", "XGBOOST_VER_MINOR"),
            ("XGBOOST_VER_PATCH", "XGBOOST_VER_PATCH"),
        ] {
            if let Some(rest) = line.trim().strip_prefix(&format!("#define {define}")) {
                let value = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .trim_end_matches("/*")
                    .trim();
                println!("cargo:rustc-env={env_key}={value}");
            }
        }
    }
}

fn main() {
    let target = env::var("TARGET").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    // dunce::canonicalize strips the \\?\ extended-length prefix that
    // Path::canonicalize() produces on Windows, which confuses CMake's
    // file(GLOB_RECURSE) when it tries to find source files.
    let xgb_root = dunce::canonicalize(Path::new("xgboost")).unwrap_or_else(|_| {
        panic!(
            "XGBoost submodule not found at xgboost-sys/xgboost. \
             Run: git submodule update --init --recursive"
        )
    });

    emit_version_env(&xgb_root);

    // ── bindgen: generate the C API bindings ────────────────────────────────
    let wrapper_h = xgb_root.join("include").join("xgboost").join("c_api.h");
    let bindings = bindgen::Builder::default()
        .header(wrapper_h.to_string_lossy())
        .clang_arg(format!("-I{}", xgb_root.join("include").display()))
        .clang_arg(format!("-I{}", xgb_root.join("dmlc-core").join("include").display()))
        .generate()
        .expect("Unable to generate bindings.");

    let out_path = PathBuf::from(&out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings.");

    // ── Build XGBoost from source as a static, CPU-only library ─────────────
    //
    // This crate only supports building libxgboost from source: there is no
    // prebuilt-binary path. A from-source build is the only way to guarantee a
    // consistent ABI with the consuming Rust binary — in particular the MSVC
    // C runtime on Windows (see FORCE_SHARED_CRT below).
    let mut dst = cmake::Config::new(&xgb_root);
    dst.generator("Ninja");
    dst.define("CMAKE_BUILD_TYPE", "RelWithDebInfo");
    // Always a static, self-contained archive — no xgboost.dll/.so at runtime.
    dst.define("BUILD_STATIC_LIB", "ON");
    dst.define("BUILD_SHARED_LIBS", "OFF");
    // CPU-only: no CUDA/NCCL. Keeping these OFF on every translation unit also
    // avoids the HostDeviceVector CPU-vs-GPU ABI split.
    dst.define("USE_CUDA", "OFF");
    dst.define("USE_NCCL", "OFF");
    // Write the built archive into the CMake binary dir (under OUT_DIR) instead
    // of XGBoost's default of the source tree. Without this, `cargo package`
    // fails verification because build.rs would modify the packaged source
    // ("Source directory was modified by build.rs"). The lib is still installed
    // to OUT_DIR/lib, which is where we link from.
    dst.define("KEEP_BUILD_ARTIFACTS_IN_BINARY_DIR", "ON");

    if target.contains("windows") {
        // CRITICAL: rustc's *-pc-windows-msvc target links the *dynamic* CRT
        // (/MD -> MSVCRT/ucrtbase). XGBoost's CMake defaults to the *static*
        // CRT (/MT -> LIBCMT) when BUILD_STATIC_LIB is on. Mixing the two gives
        // each side its own heap and its own copy of the C++ STL state, which
        // corrupts std::shared_ptr control blocks at runtime (access violation
        // c0000005). FORCE_SHARED_CRT=ON makes XGBoost (and dmlc) build /MD to
        // match rustc.
        dst.define("FORCE_SHARED_CRT", "ON");
        // MSVC's OpenMP runtime (vcomp140.dll) cannot be linked statically, so
        // building a self-contained .lib requires OpenMP off. XGBoost then runs
        // single-threaded on Windows.
        dst.define("USE_OPENMP", "OFF");
    } else if target.contains("android") {
        // Cross-compile using the Android NDK toolchain.
        //
        // `cargo ndk` (https://github.com/bbqsrc/cargo-ndk) already resolves the NDK
        // location for every invocation it makes and exposes the toolchain file
        // directly via `CARGO_NDK_CMAKE_TOOLCHAIN_PATH` — prefer that so `cargo ndk`
        // builds work with no extra environment setup. Fall back to
        // `ANDROID_NDK_HOME`/`NDK_HOME` for callers that invoke cargo directly
        // (e.g. `cargo build --target aarch64-linux-android`).
        let toolchain_file = match env::var("CARGO_NDK_CMAKE_TOOLCHAIN_PATH") {
            Ok(path) => path,
            Err(_) => {
                let ndk_home = env::var("ANDROID_NDK_HOME").or_else(|_| env::var("NDK_HOME")).expect(
                    "ANDROID_NDK_HOME or NDK_HOME must be set for Android builds \
                         (or build via `cargo ndk`, which sets CARGO_NDK_CMAKE_TOOLCHAIN_PATH)",
                );
                format!("{}/build/cmake/android.toolchain.cmake", ndk_home)
            }
        };

        let abi = if target.contains("aarch64") {
            "arm64-v8a"
        } else if target.contains("armv7") {
            "armeabi-v7a"
        } else if target.contains("x86_64") {
            "x86_64"
        } else {
            "x86"
        };

        dst.define("CMAKE_TOOLCHAIN_FILE", &toolchain_file);
        dst.define("ANDROID_ABI", abi);
        // API 26 (Android 8.0+) minimum: pthread_getname_np (used by XGBoost's
        // threading_utils.cc) was introduced there.
        dst.define("ANDROID_PLATFORM", "android-26");
        // OpenMP is not available from the NDK.
        dst.define("USE_OPENMP", "OFF");
    } else if target.contains("apple") {
        // macOS: XGBoost's patch_openmp_path_macos() adds a POST_BUILD step that
        // runs install_name_tool on libxgboost.dylib, but static builds only produce
        // libxgboost.a, so the command fails. Disabling OpenMP avoids that code path
        // without modifying the upstream submodule.
        dst.define("USE_OPENMP", "OFF");
    } else {
        // Linux: OpenMP is available from the system.
        dst.define("USE_OPENMP", "ON");
    }

    let dst = dst.build();

    println!("cargo:rustc-link-search=native={}", dst.display());
    println!("cargo:rustc-link-search=native={}", dst.join("lib").display());
    println!("cargo:rustc-link-search=native={}", dst.join("lib64").display());

    println!("cargo:rustc-link-lib=static=xgboost");
    println!("cargo:rustc-link-lib=static=dmlc");

    // ── Link the C++ runtime, OpenMP, and platform libraries ────────────────
    // Use the TARGET triple (the cross-compilation target), not cfg!() which
    // reflects the build host.
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
    } else if target.contains("android") {
        // libc++ is linked statically so the binary is fully self-contained.
        // cargo:rustc-link-arg does NOT propagate through dependency crates, so
        // copy the NDK's libc++ static archives into the deps/ directory that
        // Rust already searches, then link them by name.
        let deps_path_buf = dunce::canonicalize(Path::new(&format!("{}/../../../deps", out_dir))).unwrap();
        let sysroot_lib = ndk_sysroot_lib_dir(&target);

        for archive in &["libc++_static.a", "libc++abi.a"] {
            let src = sysroot_lib.join(archive);
            let dst = deps_path_buf.join(archive);
            if src.exists() && !dst.exists() {
                fs::copy(&src, &dst).unwrap_or_else(|e| panic!("Failed to copy {archive} from NDK sysroot: {e}"));
            }
        }
        println!("cargo:rustc-link-search=native={}", deps_path_buf.display());
        println!("cargo:rustc-link-lib=static=c++_static");
        println!("cargo:rustc-link-lib=static=c++abi");
    } else if target.contains("windows") {
        // Transitive dependencies of statically-linked XGBoost/dmlc-core.
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=Dbghelp");
    } else {
        // Linux.
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=stdc++fs");
        println!("cargo:rustc-link-lib=dylib=gomp");
    }
}

/// Maps a Rust target triple to the NDK sysroot library directory name.
///
/// The NDK sysroot for 32-bit ARM is named `arm-linux-androideabi`, but the
/// Rust target triple is `armv7-linux-androideabi`.  All other Android triples
/// match their NDK sysroot directory name exactly.
fn ndk_sysroot_triple(target: &str) -> &str {
    if target.starts_with("armv7") && target.contains("android") {
        "arm-linux-androideabi"
    } else {
        target
    }
}

/// Returns the NDK sysroot lib directory for `target`, e.g.
/// `.../toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/lib/aarch64-linux-android`.
///
/// `cargo ndk` sets `CARGO_NDK_SYSROOT_LIBS_PATH` to exactly this directory, so that
/// is checked first. Otherwise falls back to `ANDROID_NDK_HOME` / `NDK_HOME` by
/// walking the `prebuilt/` directory for the host entry.
///
/// Note: `NDK_PATH` is *not* set by cargo-ndk itself — it is a convention from this
/// project's own shell environment (points at the toolchain's `bin/` dir) — but is
/// still honored here for callers that rely on it.
fn ndk_sysroot_lib_dir(target: &str) -> PathBuf {
    let sysroot_triple = ndk_sysroot_triple(target);
    // Set directly by cargo-ndk to the exact sysroot lib dir for the current target.
    if let Ok(sysroot_libs) = env::var("CARGO_NDK_SYSROOT_LIBS_PATH") {
        return PathBuf::from(sysroot_libs);
    }
    // …/toolchains/llvm/prebuilt/<host>/bin
    if let Ok(ndk_path) = env::var("NDK_PATH") {
        return Path::new(&ndk_path)
            .parent()
            .unwrap()
            .join("sysroot/usr/lib")
            .join(sysroot_triple);
    }
    // Fallback: walk the prebuilt/ directory for the first host entry.
    if let Ok(ndk_home) = env::var("ANDROID_NDK_HOME").or_else(|_| env::var("NDK_HOME")) {
        let prebuilt = Path::new(&ndk_home).join("toolchains/llvm/prebuilt");
        if let Ok(mut entries) = std::fs::read_dir(&prebuilt) {
            if let Some(Ok(entry)) = entries.next() {
                return entry.path().join("sysroot/usr/lib").join(sysroot_triple);
            }
        }
    }
    panic!(
        "Cannot locate NDK sysroot for target '{target}'. \
         Set ANDROID_NDK_HOME or NDK_HOME, or use cargo-ndk."
    );
}
