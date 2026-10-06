fn process(values: Vec<i32>) -> Vec<i32> {
    values
        .iter()
        .filter(|x| **x > 0)
        .map(|x| **x * 2)
        .collect()
}
