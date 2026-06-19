/// Static-link integration tests.
///
/// XGBoost is always built from source as a static, CPU-only library. These
/// tests build the `smoke` example and verify that:
///   * the resulting binary has no *dynamic* dependency on xgboost, and
///   * (Windows) the built `xgboost.lib` uses the *dynamic* MSVC CRT (/MD),
///     matching rustc — a /MT mismatch is what caused the historical
///     access-violation (c0000005) crash.
///
/// These tests compile XGBoost from source and are intentionally slow (~1-2 min).
/// They are skipped automatically when the required tools are unavailable.
///
/// Prerequisites:
///   Windows – cmake, ninja, MSVC (cl.exe), and llvm-readobj on PATH
///             (llvm-readobj ships with LLVM; cmake/ninja via winget or VS installer)
///   Linux   – cmake, ninja, a C++ compiler, and libclang-dev
#[cfg(any(target_os = "linux", target_os = "windows"))]
use xshell::{cmd, Shell};

#[cfg(any(target_os = "linux", target_os = "windows"))]
const TARGET_DIR: &str = "target/static-link-test";

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn tool_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Build `cargo build --example smoke` and return the path to the produced binary.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn build_smoke(sh: &Shell) -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let ext = if cfg!(target_os = "windows") { ".exe" } else { "" };
    let binary = format!("{TARGET_DIR}/debug/examples/smoke{ext}");

    cmd!(
        sh,
        "cargo build
            --manifest-path {manifest}/Cargo.toml
            --example smoke
            --target-dir {TARGET_DIR}"
    )
    .run()
    .expect("cargo build failed");

    std::path::PathBuf::from(&binary)
}

/// Recursively search `dir` for the first file named `name`.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn find_file(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}

// ── Windows ────────────────────────────────────────────────────────────────

/// The smoke binary must not import `xgboost.dll` — XGBoost is linked statically.
#[test]
#[cfg(target_os = "windows")]
fn static_link_windows_no_xgboost_dll() {
    if !tool_available("cmake") || !tool_available("ninja") {
        eprintln!("skipping static_link_windows_no_xgboost_dll: cmake and ninja must be on PATH");
        return;
    }
    let Some(llvm_readobj) = find_llvm_readobj() else {
        eprintln!("skipping static_link_windows_no_xgboost_dll: llvm-readobj not found; install LLVM (winget install LLVM.LLVM)");
        return;
    };

    let sh = Shell::new().unwrap();
    let binary = build_smoke(&sh);

    let imports = cmd!(sh, "{llvm_readobj} --coff-imports {binary}")
        .read()
        .expect("llvm-readobj failed");

    assert!(
        !imports.to_ascii_lowercase().contains("xgboost.dll"),
        "binary has a dynamic dependency on xgboost.dll — static link failed:\n{imports}"
    );
}

/// The built `xgboost.lib` must reference the *dynamic* MSVC CRT (`MSVCRT`),
/// not the static CRT (`LIBCMT`). rustc links the dynamic CRT, so a static-CRT
/// XGBoost would give each side its own heap/STL state and corrupt memory at
/// runtime (the original c0000005 crash). This is the regression test for that
/// fix (FORCE_SHARED_CRT=ON in build.rs).
#[test]
#[cfg(target_os = "windows")]
fn windows_xgboost_lib_uses_dynamic_crt() {
    if !tool_available("cmake") || !tool_available("ninja") {
        eprintln!("skipping windows_xgboost_lib_uses_dynamic_crt: cmake and ninja must be on PATH");
        return;
    }
    let Some(llvm_readobj) = find_llvm_readobj() else {
        eprintln!("skipping windows_xgboost_lib_uses_dynamic_crt: llvm-readobj not found; install LLVM (winget install LLVM.LLVM)");
        return;
    };

    let sh = Shell::new().unwrap();
    // Ensure the lib has been built.
    let _ = build_smoke(&sh);

    let target_root = std::path::Path::new(TARGET_DIR);
    let lib =
        find_file(target_root, "xgboost.lib").expect("could not locate built xgboost.lib under the test target dir");

    let directives = cmd!(sh, "{llvm_readobj} --coff-directives {lib}")
        .read()
        .expect("llvm-readobj failed");
    let directives = directives.to_ascii_lowercase();

    assert!(
        directives.contains("defaultlib:\"msvcrt\"") || directives.contains("defaultlib:msvcrt"),
        "xgboost.lib does not reference the dynamic CRT (MSVCRT); FORCE_SHARED_CRT may be off:\n{directives}"
    );
    assert!(
        !directives.contains("libcmt"),
        "xgboost.lib references the static CRT (LIBCMT) — this is the /MT-vs-/MD mismatch that crashes at runtime:\n{directives}"
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

// ── Linux ──────────────────────────────────────────────────────────────────

/// The smoke binary must not list `libxgboost` in its `ldd` output — XGBoost is
/// linked statically.
#[test]
#[cfg(target_os = "linux")]
fn static_link_linux_no_libxgboost_so() {
    if !tool_available("cmake") || !tool_available("ninja") {
        eprintln!("skipping static_link_linux_no_libxgboost_so: cmake and ninja must be on PATH");
        return;
    }

    let sh = Shell::new().unwrap();
    let binary = build_smoke(&sh);

    let deps = cmd!(sh, "ldd {binary}").read().expect("ldd failed");

    assert!(
        !deps.to_ascii_lowercase().contains("libxgboost"),
        "binary has a dynamic dependency on libxgboost — static link failed:\n{deps}"
    );
}
