[![Actions Status](https://github.com/marcomq/rust-xgboost/workflows/Macos/badge.svg)](https://github.com/marcomq/rust-xgboost/actions/workflows/macos.yml)
[![Actions Status](https://github.com/marcomq/rust-xgboost/workflows/Linux/badge.svg)](https://github.com/marcomq/rust-xgboost/actions/workflows/linux.yml)
[![Actions Status](https://github.com/marcomq/rust-xgboost/workflows/Windows/badge.svg)](https://github.com/marcomq/rust-xgboost/actions/workflows/windows.yml)


# rust-xgboost

Rust bindings for the [XGBoost](https://xgboost.ai) gradient boosting library.

This is a fork of <https://github.com/davechallis/rust-xgboost> updated to XGBoost 3.0
and extended with prebuilt library support across multiple platforms.

## Requirements

The `use_prebuilt_xgb` feature (enabled by default) uses committed static or dynamic archives
from `xgboost-sys/lib/<platform>/` — no internet download, no CMake required at build time.

| Platform          | Library type     | Notes                                                    |
|-------------------|------------------|----------------------------------------------------------|
| Linux x86\_64     | static `.a`      | Fully self-contained binary; no runtime `.so` needed     |
| Linux arm64       | static `.a`      | Fully self-contained binary; no runtime `.so` needed     |
| macOS arm64       | dynamic `.dylib` | Requires `brew install libomp`                           |
| Windows x86\_64   | dynamic `.dll`   | Must be present at runtime                               |
| Android arm64-v8a | static `.a`      | API level 26+; see [Android](#android-arm64-v8a) section |

Additional system dependency: `libclang-dev` is required by `bindgen` at build time:

```bash
# Debian / Ubuntu
apt install -y libclang-dev

# macOS
brew install llvm
```

## Use prebuilt XGBoost library or build it

XGBoost is complex to compile, especially with GPU support. The `use_prebuilt_xgb` feature
(default) uses archives committed to this repository under `xgboost-sys/lib/`.
Set `$XGBOOST_LIB_DIR` to point at a custom directory if you need a different build.

If you prefer to use XGBoost from Homebrew (which may include GPU support):
```bash
export XGBOOST_LIB_DIR=${HOMEBREW_PREFIX}/opt/xgboost/lib
```

To build XGBoost from source at compile time, disable the default feature and enable
`local_build`:
```toml
xgb = { version = "3", default-features = false, features = ["local_build"] }
```
This requires `cmake` and `ninja-build`.  After cloning, initialize the submodule first:
```bash
git submodule update --init --recursive
```

macOS local build dependencies:
```bash
brew install libomp cmake ninja llvm
```

### Feature flags

| Feature            | Default | Description                                                                                                                                                                                                                      |
|--------------------|---------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `use_prebuilt_xgb` | ✅       | Use committed prebuilt archives; copies them to the Cargo `deps/` dir at build time                                                                                                                                              |
| `local_build`      | ❌       | Build XGBoost from source via CMake/Ninja at compile time                                                                                                                                                                        |
| `static_link`      | ❌       | Link libxgboost statically. Always implied for Android. On Linux (and macOS/Windows), enables self-contained binaries suitable for `cargo-deb` packaging. When combined with `local_build`, sets `BUILD_STATIC_LIB=ON` in CMake. |
| `cuda`             | ❌       | Enable CUDA/GPU support (requires a local CUDA toolkit)                                                                                                                                                                          |

### Supported platforms

| Platform          | `use_prebuilt_xgb` | `local_build`            |
|-------------------|--------------------|--------------------------|
| macOS (arm64)     | ✅                  | ✅                        |
| Linux x86\_64     | ✅                  | ✅                        |
| Linux arm64       | ✅                  | ✅                        |
| Windows x86\_64   | ✅                  | ⚠️ manual steps required |
| Android arm64-v8a | ✅                  | ✅ (via `cargo-ndk`)      |

## Linux — static linking

On Linux, the default (`use_prebuilt_xgb` without `static_link`) links **dynamically** against
the committed `libxgboost.so`.  Enable the `static_link` feature to link against
`libxgboost.a` instead, producing a binary with no runtime dependency on `libxgboost.so`.
This is the recommended approach when packaging with `cargo-deb` or deploying to systems where
`libxgboost.so` is not installed.

```toml
xgb = { version = "3", features = ["static_link"] }
```

`libgomp` (OpenMP) is still linked dynamically in both cases.  On Debian/Ubuntu it is provided
by the `libgomp1` package, which is present by default.

#### Rebuilding the prebuilt Linux archives

The prebuilt archives were built with `cmake -DBUILD_STATIC_LIB=ON -DUSE_OPENMP=ON
-DCMAKE_BUILD_TYPE=Release` inside a Debian 12 container and then stripped with
`llvm-strip --strip-debug`.  To rebuild them (e.g. after updating the XGBoost submodule):

```bash
# Requires Docker with buildx support.  Run from the repository root.
xgboost-sys/scripts/build-linux-static.sh
```

This produces stripped `libxgboost.a` and `libdmlc.a` for both `linux_amd64` and
`linux_arm64`, copies them into `xgboost-sys/lib/`, and restores the submodule to a clean
state.  Commit the resulting files.

Alternatively, if you have a Linux environment with a Rust toolchain, CMake, Ninja, and
`libclang-dev` available, you can use Cargo directly:

```bash
cargo build \
  --manifest-path xgboost-sys/Cargo.toml \
  --no-default-features \
  --features local_build,static_link
```

The `static_link` feature sets `BUILD_STATIC_LIB=ON` in the CMake invocation and ensures the
final link uses `cargo:rustc-link-lib=static=xgboost`.

## Documentation

* [Documentation](https://docs.rs/xgboost)

Basic usage example:

```rust
extern crate xgb;

use xgb::{parameters, DMatrix, Booster};

fn main() {
    // training matrix with 5 training examples and 3 features
    let x_train = &[1.0, 1.0, 1.0,
                    1.0, 1.0, 0.0,
                    1.0, 1.0, 1.0,
                    0.0, 0.0, 0.0,
                    1.0, 1.0, 1.0];
    let num_rows = 5;
    let y_train = &[1.0, 1.0, 1.0, 0.0, 1.0];

    // convert training data into XGBoost's matrix format
    let mut dtrain = DMatrix::from_dense(x_train, num_rows).unwrap();

    // set ground truth labels for the training matrix
    dtrain.set_labels(y_train).unwrap();

    // test matrix with 1 row
    let x_test = &[0.7, 0.9, 0.6];
    let num_rows = 1;
    let y_test = &[1.0];
    let mut dtest = DMatrix::from_dense(x_test, num_rows).unwrap();
    dtest.set_labels(y_test).unwrap();

    // configure objectives, metrics, etc.
    let learning_params = parameters::learning::LearningTaskParametersBuilder::default()
        .objective(parameters::learning::Objective::BinaryLogistic)
        .build().unwrap();

    // configure the tree-based learning model's parameters
    let tree_params = parameters::tree::TreeBoosterParametersBuilder::default()
            .max_depth(2)
            .eta(1.0)
            .build().unwrap();

    // overall configuration for Booster
    let booster_params = parameters::BoosterParametersBuilder::default()
        .booster_type(parameters::BoosterType::Tree(tree_params))
        .learning_params(learning_params)
        .verbose(true)
        .build().unwrap();

    // specify datasets to evaluate against during training
    let evaluation_sets = &[(&dtrain, "train"), (&dtest, "test")];

    // overall configuration for training/evaluation
    let params = parameters::TrainingParametersBuilder::default()
        .dtrain(&dtrain)                         // dataset to train with
        .boost_rounds(2)                         // number of training iterations
        .booster_params(booster_params)          // model parameters
        .evaluation_sets(Some(evaluation_sets)) // optional datasets to evaluate against in each iteration
        .build().unwrap();

    // train model, and print evaluation data
    let bst = Booster::train(&params).unwrap();

    println!("{:?}", bst.predict(&dtest).unwrap());
}
```

See the [examples](https://github.com/marcomq/rust-xgboost/tree/master/examples) directory for
more detailed examples of different features.

## Status

The version number is just an indicator that xboost 3.0.0 is used.

This is still a very early stage of development, so the API is changing as usability issues occur,
or new features are supported. This is still expected to be compatible to an earlier rust-xgboost library.

Builds against XGBoost 3.0.0.

## Android (arm64-v8a)

A prebuilt static `libxgboost.a` (API level 26+, OpenMP disabled) is bundled in
`xgboost-sys/lib/android_arm64/`. The `use_prebuilt_xgb` feature (enabled by default) picks it
up automatically when building for `aarch64-linux-android`.

### Prerequisites

1. Install [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk):
   ```bash
   cargo install cargo-ndk
   ```
2. Install the Android target:
   ```bash
   rustup target add aarch64-linux-android
   ```
3. Install the Android NDK (e.g. via Android Studio SDK Manager or `sdkmanager`).
4. Set `ANDROID_NDK_HOME` to the NDK root, e.g.:
   ```bash
   export ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/<version>
   ```

### Running tests

Unit and integration tests run on a connected device or emulator. Push the test data first:

```bash
adb push xgboost-sys/xgboost/demo/data /data/local/tmp/xgboost-sys/xgboost/demo/data
```

Because `cargo-ndk-runner` pushes every doctest binary to the same device path, doctests must
be run single-threaded to avoid collisions:

```bash
# Unit / integration tests (parallel is fine)
cargo ndk -t arm64-v8a -P 26 test --lib

# Doctests (must be single-threaded)
cargo ndk -t arm64-v8a -P 26 test --doc -- --test-threads=1
```

## Windows — GPU support

To obtain a `.lib` and `.dll` from pip using a VS Developer Command Prompt:

```bat
python3 -m venv .venv
.venv\Scripts\activate.bat
pip install xgboost
pip show xgboost
:: check the Location entry
copy {Location}\xgboost.dll .
gendef xgboost.dll
lib /def:xgboost.def /machine:x64 /out:xgboost.lib
```


