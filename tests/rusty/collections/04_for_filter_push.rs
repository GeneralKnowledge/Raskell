fn positives_doubled(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for x in values {
        if x > 0 {
            result.push(x * 2);
        }
    }
    result
}
