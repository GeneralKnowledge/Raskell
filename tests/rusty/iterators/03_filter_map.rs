fn parseish(values: Vec<i32>) -> Vec<i32> {
    values
        .iter()
        .filter_map(|x| if **x > 0 { Some(**x) } else { None })
        .collect()
}
