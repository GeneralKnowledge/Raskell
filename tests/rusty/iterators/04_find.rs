fn first_positive(values: Vec<i32>) -> Option<i32> {
    values.iter().copied().find(|x| *x > 0)
}
