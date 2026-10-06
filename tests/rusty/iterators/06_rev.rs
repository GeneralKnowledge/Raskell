fn reverse_copy(values: Vec<i32>) -> Vec<i32> {
    values.iter().rev().copied().collect()
}
