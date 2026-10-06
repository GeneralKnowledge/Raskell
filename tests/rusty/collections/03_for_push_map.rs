fn double_all(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for value in values {
        result.push(value * 2);
    }
    result
}
