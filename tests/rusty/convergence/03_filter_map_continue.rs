fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for value in values {
        if value <= 0 {
            continue;
        }
        result.push(value * 2);
    }
    result
}
