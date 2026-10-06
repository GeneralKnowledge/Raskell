fn calculate(values: Vec<i32>) -> i32 {
    values
        .iter()
        .filter(|x| **x > 0)
        .map(|x| *x * 2)
        .sum()
}
