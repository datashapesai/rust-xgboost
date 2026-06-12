/// Static-link integration tests.
///
/// Each test builds the `smoke` example with static-linking features enabled,
/// then inspects the resulting binary's import table to verify that xgboost
/// is not present as a dynamic dependency.
///
/// These tests compile XGBoost from source and are intentionally slow (~1-2 min).
/// They are skipped automatically on unsupported platforms.
///
/// Prerequisites:
///   Windows – cmake, ninja, MSVC (cl.exe), and llvm-readobj on PATH
///             (llvm-readobj ships with LLVM; cmake/ninja via winget or VS installer)
///   Linux   – cmake, ninja, a C++ compiler, and libclang-dev
use xshell::{Shell, cmd};

const TARGET_DIR: &str = "target/static-link-test";

/// Run `cargo build --example smoke` with the supplied feature flags and return
/// the path to the produced binary.
fn build_smoke(sh: &Shell, features: &str) -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let ext = if cfg!(target_os = "windows") { ".exe" } else { "" };
    let binary = format!("{TARGET_DIR}/debug/examples/smoke{ext}");

    cmd!(
        sh,
        "cargo build
            --manifest-path {manifest}/Cargo.toml
            --no-default-features
            --features {features}
            --example smoke
            --target-dir {TARGET_DIR}"
    )
    .run()
    .expect("static cargo build failed");

    std::path::PathBuf::from(&binary)
}

// ── Windows ────────────────────────────────────────────────────────────────

/// On Windows with the `local_build` feature, xgboost is compiled as a static
/// `.lib` (with OpenMP disabled to avoid the vcomp DLL).  The smoke binary must
/// not import `xgboost.dll`.
#[test]
#[cfg(target_os = "windows")]
fn static_link_windows_no_xgboost_dll() {
    let sh = Shell::new().unwrap();

    let binary = build_smoke(&sh, "local_build");

    // Locate llvm-readobj: try PATH first, then the default LLVM install location.
    let llvm_readobj = find_llvm_readobj().expect(
        "llvm-readobj not found; install LLVM (winget install LLVM.LLVM) and ensure it is on PATH",
    );

    let imports = cmd!(sh, "{llvm_readobj} --coff-imports {binary}")
        .read()
        .expect("llvm-readobj failed");

    assert!(
        !imports.to_ascii_lowercase().contains("xgboost.dll"),
        "binary has a dynamic dependency on xgboost.dll — static link failed:\n{imports}"
    );
}

#[cfg(target_os = "windows")]
fn find_llvm_readobj() -> Option<std::path::PathBuf> {
    // Try PATH first.
    if std::process::Command::new("llvm-readobj")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(std::path::PathBuf::from("llvm-readobj"));
    }
    // Fall back to the default LLVM install locations on Windows.
    for candidate in &[
        r"C:\Program Files\LLVM\bin\llvm-readobj.exe",
        r"C:\Program Files (x86)\LLVM\bin\llvm-readobj.exe",
    ] {
        let p = std::path::Path::new(candidate);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// On Windows with `use_prebuilt_xgb` + `static_link`, the committed
/// `lib/win_amd64/xgboost.lib` and `dmlc.lib` are copied to deps and linked
/// statically — no CMake required.  The smoke binary must not import `xgboost.dll`.
#[test]
#[cfg(target_os = "windows")]
fn static_link_windows_prebuilt_no_xgboost_dll() {
    let sh = Shell::new().unwrap();

    let binary = build_smoke(&sh, "use_prebuilt_xgb,static_link");

    let llvm_readobj = find_llvm_readobj().expect(
        "llvm-readobj not found; install LLVM (winget install LLVM.LLVM) and ensure it is on PATH",
    );

    let imports = cmd!(sh, "{llvm_readobj} --coff-imports {binary}")
        .read()
        .expect("llvm-readobj failed");

    assert!(
        !imports.to_ascii_lowercase().contains("xgboost.dll"),
        "binary has a dynamic dependency on xgboost.dll — static link failed:\n{imports}"
    );
}

// ── Linux ──────────────────────────────────────────────────────────────────

/// On Linux with `local_build` + `static_link`, xgboost is compiled as a
/// static archive and linked in.  The smoke binary must not list `libxgboost`
/// in its `ldd` output.
#[test]
#[cfg(target_os = "linux")]
fn static_link_linux_no_libxgboost_so() {
    let sh = Shell::new().unwrap();

    let binary = build_smoke(&sh, "local_build,static_link");

    let deps = cmd!(sh, "ldd {binary}")
        .read()
        .expect("ldd failed");

    assert!(
        !deps.to_ascii_lowercase().contains("libxgboost"),
        "binary has a dynamic dependency on libxgboost — static link failed:\n{deps}"
    );
}
