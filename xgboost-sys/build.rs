use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const GITHUB_URL: &str = "https://github.com/marcomq/rust-xgboost/raw/refs/tags/v3.0.1/xgboost-sys/lib/";

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
    let xgb_root = dunce::canonicalize(Path::new("xgboost")).unwrap();

    emit_version_env(&xgb_root);

    let wrapper_h = xgb_root.join("include").join("xgboost").join("c_api.h");
    let bindings = bindgen::Builder::default()
        .header(wrapper_h.to_string_lossy())
        .clang_arg(format!("-I{}", xgb_root.join("include").display()))
        .clang_arg(format!("-I{}", xgb_root.join("dmlc-core").join("include").display()));

    #[cfg(feature = "cuda")]
    let bindings = bindings.clang_arg("-I/usr/local/cuda/include");
    let bindings = bindings.generate().expect("Unable to generate bindings.");

    let out_path = PathBuf::from(&out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings.");

    // Linker search path for Apple OpenMP (host check is fine here — only relevant when target is also macOS)
    if target.contains("apple") {
        println!(
            "cargo:rustc-link-search=native={}/opt/libomp/lib",
            &std::env::var("HOMEBREW_PREFIX").unwrap_or("/opt/homebrew".into())
        );
    }

    #[cfg(feature = "use_prebuilt_xgb")]
    {
        if let Ok(xgboost_lib_dir) = std::env::var("XGBOOST_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", xgboost_lib_dir);
        } else {
            let deps_path = dunce::canonicalize(Path::new(&format!("{}/../../../deps", out_dir))).unwrap();
            let deps_path = deps_path.to_string_lossy();
            println!("cargo:rustc-link-search=native={}", deps_path);

            if target.contains("apple") && target.contains("aarch64") {
                let path = format!("{GITHUB_URL}/mac_arm64");
                if !std::fs::exists(format!("{deps_path}/libxgboost.dylib")).unwrap() {
                    web_copy(
                        &format!("{path}/libxgboost.dylib"),
                        &format!("{deps_path}/libxgboost.dylib"),
                    )
                    .unwrap();
                    web_copy(&format!("{path}/libdmlc.a"), &format!("{deps_path}/libdmlc.a")).unwrap();
                }
            } else if target.contains("android") {
                if target.contains("aarch64") {
                    // Prefer local prebuilt (present in the source repo under lib/android_arm64/)
                    // when building from source. Falls back to XGBOOST_LIB_DIR (handled above)
                    // or a future GitHub download URL.
                    // We ship a static archive so libxgboost.so doesn't need to be on-device.
                    let local_lib = Path::new("lib/android_arm64/libxgboost.a");
                    if local_lib.exists() {
                        if !std::fs::exists(format!("{deps_path}/libxgboost.a")).unwrap() {
                            fs::copy(local_lib, format!("{deps_path}/libxgboost.a"))
                                .expect("Failed to copy Android arm64 libxgboost.a to deps");
                        }
                        let local_dmlc = Path::new("lib/android_arm64/libdmlc.a");
                        if local_dmlc.exists() && !std::fs::exists(format!("{deps_path}/libdmlc.a")).unwrap() {
                            fs::copy(local_dmlc, format!("{deps_path}/libdmlc.a"))
                                .expect("Failed to copy Android arm64 libdmlc.a to deps");
                        }
                    } else if !std::fs::exists(format!("{deps_path}/libxgboost.a")).unwrap() {
                        // Attempt to download from GitHub once a release asset is available there.
                        let path = format!("{GITHUB_URL}/android_arm64");
                        web_copy(&format!("{path}/libxgboost.a"), &format!("{deps_path}/libxgboost.a")).unwrap();
                        web_copy(&format!("{path}/libdmlc.a"), &format!("{deps_path}/libdmlc.a")).unwrap();
                    }
                } else {
                    panic!(
                        "Unsupported Android target '{}'. \
                         Please set $XGBOOST_LIB_DIR to a directory containing \
                         libxgboost.a built for this ABI.",
                        target
                    );
                }
            } else if target.contains("linux") {
                let arch_dir = if target.contains("aarch64") { "linux_arm64" } else { "linux_amd64" };

                #[cfg(feature = "static_link")]
                {
                    // static_link: copy the committed .a archive so the linker
                    // can produce a fully self-contained binary (e.g. cargo-deb).
                    let local_lib_path = format!("lib/{arch_dir}/libxgboost.a");
                    let local_lib = Path::new(&local_lib_path);
                    if local_lib.exists() {
                        if !std::fs::exists(format!("{deps_path}/libxgboost.a")).unwrap() {
                            fs::copy(local_lib, format!("{deps_path}/libxgboost.a"))
                                .expect("Failed to copy Linux libxgboost.a to deps");
                        }
                        let local_dmlc_path = format!("lib/{arch_dir}/libdmlc.a");
                        let local_dmlc = Path::new(&local_dmlc_path);
                        if local_dmlc.exists() && !std::fs::exists(format!("{deps_path}/libdmlc.a")).unwrap() {
                            fs::copy(local_dmlc, format!("{deps_path}/libdmlc.a"))
                                .expect("Failed to copy Linux libdmlc.a to deps");
                        }
                    } else {
                        panic!(
                            "No prebuilt libxgboost.a found at lib/{arch_dir}/libxgboost.a. \
                             Build it with `xgboost-sys/scripts/build-linux-static.sh` and \
                             commit the result, or set $XGBOOST_LIB_DIR to a directory \
                             containing a libxgboost.a built for target '{target}'."
                        );
                    }
                }
                #[cfg(not(feature = "static_link"))]
                {
                    // Default: copy the committed .so for dynamic linking.
                    let local_so_path = format!("lib/{arch_dir}/libxgboost.so");
                    let local_so = Path::new(&local_so_path);
                    if local_so.exists() {
                        if !std::fs::exists(format!("{deps_path}/libxgboost.so")).unwrap() {
                            fs::copy(local_so, format!("{deps_path}/libxgboost.so"))
                                .expect("Failed to copy Linux libxgboost.so to deps");
                        }
                        let local_dmlc_path = format!("lib/{arch_dir}/libdmlc.a");
                        let local_dmlc = Path::new(&local_dmlc_path);
                        if local_dmlc.exists() && !std::fs::exists(format!("{deps_path}/libdmlc.a")).unwrap() {
                            fs::copy(local_dmlc, format!("{deps_path}/libdmlc.a"))
                                .expect("Failed to copy Linux libdmlc.a to deps");
                        }
                    } else {
                        // Fall back to downloading from the upstream GitHub release.
                        let path = if target.contains("aarch64") {
                            format!("{GITHUB_URL}/linux_arm64")
                        } else {
                            format!("{GITHUB_URL}/linux_amd64")
                        };
                        if !std::fs::exists(format!("{deps_path}/libxgboost.so")).unwrap() {
                            web_copy(&format!("{path}/libxgboost.so"), &format!("{deps_path}/libxgboost.so")).unwrap();
                            web_copy(&format!("{path}/libdmlc.a"), &format!("{deps_path}/libdmlc.a")).unwrap();
                        }
                    }
                }
            } else if target.contains("windows") {
                let path = format!("{GITHUB_URL}/win_amd64");
                if !std::fs::exists(format!("{deps_path}/xgboost.dll")).unwrap() {
                    web_copy(&format!("{path}/xgboost.dll"), &format!("{deps_path}/xgboost.dll")).unwrap();
                    web_copy(&format!("{path}/xgboost.lib"), &format!("{deps_path}/xgboost.lib")).unwrap();
                }
            } else if let Ok(homebrew_path) = std::env::var("HOMEBREW_PREFIX") {
                let xgboost_lib_dir = format!("{}/opt/xgboost/lib", &homebrew_path);
                println!("cargo:rustc-link-search=native={}", xgboost_lib_dir);
            } else {
                panic!("Please set $XGBOOST_LIB_DIR")
            }
        }
    }

    #[cfg(feature = "local_build")]
    {
        // Compile XGBoost with CMake + Ninja.
        let mut dst = cmake::Config::new(&xgb_root);
        dst.generator("Ninja");
        dst.define("CMAKE_BUILD_TYPE", "RelWithDebInfo");

        if target.contains("android") {
            // Cross-compile using the Android NDK toolchain.
            let ndk_home = env::var("ANDROID_NDK_HOME")
                .or_else(|_| env::var("NDK_HOME"))
                .expect("ANDROID_NDK_HOME or NDK_HOME must be set for Android local_build");
            let toolchain_file = format!("{}/build/cmake/android.toolchain.cmake", ndk_home);

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
            // Use API 26 (Android 8.0+) as the minimum.  pthread_getname_np (used
            // by XGBoost's threading_utils.cc) was introduced in API 26; building
            // against an earlier API level would require patching the submodule.
            dst.define("ANDROID_PLATFORM", "android-26");
            // OpenMP is not available from the NDK; build without it.
            dst.define("USE_OPENMP", "OFF");
            dst.define("USE_CUDA", "OFF");
            dst.define("USE_NCCL", "OFF");
        }

        if target.contains("windows") {
            // Build a true static lib; disable OpenMP to avoid a vcomp DLL dependency
            // (MSVC's OpenMP runtime cannot be statically linked).
            dst.define("BUILD_STATIC_LIB", "ON");
            dst.define("BUILD_SHARED_LIBS", "OFF");
            dst.define("USE_OPENMP", "OFF");
        }

        // When static_link is requested, produce a static archive instead of
        // (or in addition to) the shared library.  This is required for the
        // final `cargo:rustc-link-lib=static=xgboost` directive to succeed.
        #[cfg(feature = "static_link")]
        if !target.contains("windows") && !target.contains("android") {
            dst.define("BUILD_STATIC_LIB", "ON");
            dst.define("BUILD_SHARED_LIBS", "OFF");
        }

        #[cfg(feature = "cuda")]
        {
            dst.define("USE_CUDA", "ON")
                .define("BUILD_WITH_CUDA", "ON")
                .define("BUILD_WITH_CUDA_CUB", "ON");
        }

        let dst = dst.build();

        println!("cargo:rustc-link-search=native={}", dst.display());
        println!("cargo:rustc-link-search=native={}", dst.join("lib").display());
        println!("cargo:rustc-link-search=native={}", dst.join("lib64").display());
        println!("cargo:rustc-link-lib=static=dmlc");

        if target.contains("windows") {
            // Transitive dependencies of statically-linked XGBoost/dmlc-core on Windows.
            println!("cargo:rustc-link-lib=ws2_32");
            println!("cargo:rustc-link-lib=Dbghelp");
        }
    }

    // Link to the appropriate C++ runtime.
    // Use TARGET env var (the cross-compilation target), not cfg!() which reflects the host.
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=dylib=omp");
    } else if target.contains("android") {
        // libxgboost.a and libdmlc.a are linked statically (no .so needed on
        // device).  libc++ is also linked statically so the binary is fully
        // self-contained.  cargo:rustc-link-arg does NOT propagate through
        // dependency crates, so we copy the NDK's libc++ static archives into
        // the same deps/ directory that Rust already searches, then link them
        // by name.
        let sysroot_lib = ndk_sysroot_lib_dir(&target);
        let deps_path = dunce::canonicalize(Path::new(&format!("{}/../../../deps", out_dir))).unwrap();

        for archive in &["libc++_static.a", "libc++abi.a"] {
            let src = sysroot_lib.join(archive);
            let dst = deps_path.join(archive);
            if src.exists() && !dst.exists() {
                fs::copy(&src, &dst).unwrap_or_else(|e| panic!("Failed to copy {archive} from NDK sysroot: {e}"));
            }
        }
        println!("cargo:rustc-link-search=native={}", deps_path.display());
        println!("cargo:rustc-link-lib=static=c++_static");
        println!("cargo:rustc-link-lib=static=c++abi");
    } else if target.contains("linux") {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=stdc++fs");
        println!("cargo:rustc-link-lib=dylib=gomp");
    }

    // Android is always static (no .so prebuilt exists for Android).
    // Windows local_build is always static (MSVC's OpenMP runtime cannot be statically linked,
    // so we disable OpenMP and produce a self-contained .lib).
    // Linux and other platforms respect the `static_link` feature flag.
    let windows_local_build = cfg!(feature = "local_build") && target.contains("windows");
    if target.contains("android") || cfg!(feature = "static_link") || windows_local_build {
        println!("cargo:rustc-link-lib=static=xgboost");
        println!("cargo:rustc-link-lib=static=dmlc");
    } else {
        println!("cargo:rustc-link-lib=dylib=xgboost");
    }

    #[cfg(feature = "cuda")]
    {
        println!("cargo:rustc-link-search={}", "/usr/local/cuda/lib64");
        println!("cargo:rustc-link-lib=static=cudart_static");
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Returns the NDK sysroot lib directory for `target`, e.g.
/// `.../toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/lib/aarch64-linux-android`.
///
/// Uses `NDK_PATH` (set by cargo-ndk) or falls back to `ANDROID_NDK_HOME` / `NDK_HOME`.
fn ndk_sysroot_lib_dir(target: &str) -> PathBuf {
    // NDK_PATH = …/toolchains/llvm/prebuilt/<host>/bin  (set by cargo-ndk)
    if let Ok(ndk_path) = env::var("NDK_PATH") {
        return Path::new(&ndk_path)
            .parent()
            .unwrap()
            .join("sysroot/usr/lib")
            .join(target);
    }
    // Fallback: walk the prebuilt/ directory for the first host entry.
    if let Ok(ndk_home) = env::var("ANDROID_NDK_HOME").or_else(|_| env::var("NDK_HOME")) {
        let prebuilt = Path::new(&ndk_home).join("toolchains/llvm/prebuilt");
        if let Ok(mut entries) = std::fs::read_dir(&prebuilt) {
            if let Some(Ok(entry)) = entries.next() {
                return entry.path().join("sysroot/usr/lib").join(target);
            }
        }
    }
    panic!(
        "Cannot locate NDK sysroot for target '{target}'. \
         Set ANDROID_NDK_HOME or NDK_HOME, or use cargo-ndk."
    );
}

#[cfg(feature = "use_prebuilt_xgb")]
fn web_copy(web_src: &str, target: &str) -> Result<()> {
    dbg!(&web_src);
    let resp = reqwest::blocking::get(web_src)?;
    let body = resp.bytes()?;
    std::fs::write(target, &body)?;
    Ok(())
}
