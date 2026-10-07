fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for value in values {
        let doubled = value * 2;
        if doubled > 0 {
            result.push(doubled);
        }
    }
    result
}
