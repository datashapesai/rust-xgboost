#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

/// Major version of the XGBoost headers compiled against (from `version_config.h`).
pub const COMPILE_VER_MAJOR: &str = env!("XGBOOST_VER_MAJOR");
/// Minor version of the XGBoost headers compiled against (from `version_config.h`).
pub const COMPILE_VER_MINOR: &str = env!("XGBOOST_VER_MINOR");
/// Patch version of the XGBoost headers compiled against (from `version_config.h`).
pub const COMPILE_VER_PATCH: &str = env!("XGBOOST_VER_PATCH");

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    /// Reads one boolean flag (e.g. `USE_OPENMP`) from the JSON that `XGBuildInfo` returns.
    fn build_info_flag(flag: &str) -> bool {
        let mut raw: *const std::os::raw::c_char = std::ptr::null();
        assert_eq!(unsafe { XGBuildInfo(&mut raw) }, 0);
        let info = unsafe { CStr::from_ptr(raw) }.to_str().unwrap();
        let needle = format!("\"{flag}\":");
        let rest = info
            .split_once(needle.as_str())
            .unwrap_or_else(|| panic!("{flag} missing from {info}"))
            .1;
        rest.trim_start().starts_with("true")
    }

    #[test]
    fn openmp_matches_build_configuration() {
        // build.rs decides per target whether OpenMP is on (Linux and Apple Silicon
        // macOS) and exports its decision; the compiled library must agree.
        let expected = env!("XGBOOST_SYS_EXPECT_OPENMP") == "1";
        assert_eq!(build_info_flag("USE_OPENMP"), expected);
    }

    #[test]
    fn read_matrix() {
        let dmat_train = "xgboost/demo/data/agaricus.txt.train";

        let silent = 0;
        let mut handle = std::ptr::null_mut();
        let fname = std::ffi::CString::new(dmat_train).unwrap();
        let ret_val = unsafe { XGDMatrixCreateFromFile(fname.as_ptr(), silent, &mut handle) };
        assert_eq!(ret_val, 0);

        let mut num_rows = 0;
        let ret_val = unsafe { XGDMatrixNumRow(handle, &mut num_rows) };
        assert_eq!(ret_val, 0);
        assert_eq!(num_rows, 6513);

        let mut num_cols = 0;
        let ret_val = unsafe { XGDMatrixNumCol(handle, &mut num_cols) };
        assert_eq!(ret_val, 0);
        assert_eq!(num_cols, 126);

        let ret_val = unsafe { XGDMatrixFree(handle) };
        assert_eq!(ret_val, 0);
    }
}
