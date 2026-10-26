# Unreleased (3.1.3+dsai)

* Apple Silicon macOS (`aarch64-apple-darwin`) now builds XGBoost with OpenMP by default, so
  `BoosterParametersBuilder::threads` takes effect there. Homebrew's static `libomp.a` is linked,
  leaving no `libomp.dylib` dependency in the final binary. Requires `brew install libomp`; the
  build fails with instructions if it is missing. Set `XGBOOST_SYS_DISABLE_OPENMP=1` to build
  single-threaded, or `LIBOMP_PREFIX` to use a libomp outside Homebrew's default location.
* The `install_name_tool` post-build step in XGBoost's CMake is skipped through
  `CMAKE_PROJECT_INCLUDE` (`xgboost-sys/omp_override.cmake`) rather than a submodule patch.
* Intel macOS, iOS, Windows and Android still build without OpenMP; Linux is unchanged.
* `xgboost-sys` build script now declares `rerun-if-changed`/`rerun-if-env-changed` inputs.
* New tests: `XGBuildInfo` reports the expected `USE_OPENMP`, and training gives the same
  predictions with 1 and 4 threads.

# 0.1.4 (2019-03-05)

* `Booster::load_buffer` method added (thanks [jonathanstrong](https://github.com/jonathanstrong))
