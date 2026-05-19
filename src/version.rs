use std::fmt;

/// A major.minor.patch version triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct XGBVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl fmt::Display for XGBVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The compile-time and runtime versions of the XGBoost C library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Version of the XGBoost headers that this crate was compiled against.
    pub compile_time: XGBVersion,
    /// Version of the XGBoost shared library loaded at runtime.
    pub runtime: XGBVersion,
}

impl Version {
    /// Returns `true` when the compile-time and runtime versions match exactly.
    pub fn is_consistent(&self) -> bool {
        self.compile_time == self.runtime
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "compile-time: {}, runtime: {}", self.compile_time, self.runtime)
    }
}

/// Returns the compile-time version (from `version_config.h`) and the runtime
/// version reported by `XGBoostVersion()` from the loaded shared library.
///
/// # Example
///
/// ```
/// let v = xgb::xgb_version();
/// println!("{v}");
/// if !v.is_consistent() {
///     eprintln!("Warning: XGBoost header/library version mismatch: {v}");
/// }
/// ```
pub fn xgb_version() -> Version {
    // Compile-time constants exposed by xgboost-sys from version_config.h.
    let compile_time = XGBVersion {
        major: xgboost_sys::COMPILE_VER_MAJOR.parse().unwrap_or(0),
        minor: xgboost_sys::COMPILE_VER_MINOR.parse().unwrap_or(0),
        patch: xgboost_sys::COMPILE_VER_PATCH.parse().unwrap_or(0),
    };

    // Runtime version from the loaded shared library.
    let mut major: std::os::raw::c_int = 0;
    let mut minor: std::os::raw::c_int = 0;
    let mut patch: std::os::raw::c_int = 0;
    unsafe {
        xgboost_sys::XGBoostVersion(&mut major, &mut minor, &mut patch);
    }
    let runtime = XGBVersion {
        major: major as u32,
        minor: minor as u32,
        patch: patch as u32,
    };

    Version { compile_time, runtime }
}
