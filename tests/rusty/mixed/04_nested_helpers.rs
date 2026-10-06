fn square(x: i32) -> i32 { x * x }
fn sum_squares(values: Vec<i32>) -> i32 {
    values.iter().map(|x| square(**x)).sum()
}
