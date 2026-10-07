fn transform(values: Vec<i32>) -> Vec<i32> {
    values.into_iter().map(|x| x * 2).collect()
}
