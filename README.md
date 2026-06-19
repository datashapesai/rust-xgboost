[![Actions Status](https://github.com/marcomq/rust-xgboost/workflows/Macos/badge.svg)](https://github.com/marcomq/rust-xgboost/actions/workflows/macos.yml)
[![Actions Status](https://github.com/marcomq/rust-xgboost/workflows/Linux/badge.svg)](https://github.com/marcomq/rust-xgboost/actions/workflows/linux.yml)
[![Actions Status](https://github.com/marcomq/rust-xgboost/workflows/Windows/badge.svg)](https://github.com/marcomq/rust-xgboost/actions/workflows/windows.yml)


# rust-xgboost

Rust bindings for the [XGBoost](https://xgboost.ai) gradient boosting library.

This is a fork of <https://github.com/davechallis/rust-xgboost> updated to XGBoost 3.0.

XGBoost is **always built from source** as a **static, CPU-only** library. There are no
prebuilt-binary, dynamic-linking, or CUDA build options — this keeps the ABI consistent with
the consuming Rust binary on every platform (the original motivation: a static-vs-dynamic CRT
mismatch on Windows caused runtime memory corruption). The build produces a self-contained
binary with no `xgboost.dll`/`.so`/`.dylib` dependency at runtime.

## Requirements

A from-source build needs the following at compile time on every platform:

* the XGBoost submodule — after cloning, run `git submodule update --init --recursive`
* `cmake` and `ninja`
* a C++ toolchain (MSVC `cl.exe` on Windows; `g++`/`clang++` elsewhere)
* `libclang` for `bindgen`

Per-platform notes:

| Platform        | OpenMP            | Extra setup                                  |
|-----------------|-------------------|----------------------------------------------|
| Linux x86\_64   | system `libgomp1` | `apt install -y libclang-dev cmake ninja-build` |
| Linux arm64     | system `libgomp1` | as above                                     |
| macOS arm64     | Homebrew `libomp` | `brew install libomp cmake ninja llvm`       |
| Windows x86\_64 | disabled¹         | MSVC, `cmake`, `ninja` on PATH               |
| Android arm64   | disabled          | `cargo-ndk`; set `ANDROID_NDK_HOME`          |

¹ MSVC's OpenMP runtime (`vcomp140.dll`) cannot be linked statically, so OpenMP is disabled on
Windows and XGBoost runs single-threaded there.

## Usage

No build features are required — just depend on the crate and build:

```toml
xgb = "3"
```

```bash
git submodule update --init --recursive
cargo build
```

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

XGBoost is cross-compiled from source for `aarch64-linux-android` (API level 26+, OpenMP
disabled) via the NDK toolchain. `libc++` is linked statically so the binary is self-contained.

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



