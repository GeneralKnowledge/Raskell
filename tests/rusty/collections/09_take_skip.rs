fn middle(values: Vec<i32>) -> Vec<i32> {
    values.iter().skip(1).take(3).copied().collect()
}
