fn indexed_sum(values: Vec<i32>) -> i32 {
    values.iter().enumerate().map(|(i, x)| (i as i32) * **x).sum()
}
