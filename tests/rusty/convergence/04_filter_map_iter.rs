fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
    values
        .iter()
        .map(|x| *x * 2)
        .filter(|x| *x > 0)
        .copied()
        .collect()
}
