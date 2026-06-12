// Minimal XGBoost API exercise used by the static-link integration test.
// Deliberately kept trivial so it succeeds without any training data or model.
fn main() {
    let data = vec![1.0f32, 2.0, 3.0, 4.0];
    let _mat = xgb::DMatrix::from_dense(&data, 2).expect("DMatrix::from_dense failed");
    println!("ok");
}
