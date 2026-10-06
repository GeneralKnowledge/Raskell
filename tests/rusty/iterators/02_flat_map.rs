fn flatten_pairs(values: Vec<i32>) -> Vec<i32> {
    values.iter().flat_map(|x| vec![**x, **x * 2]).collect()
}
