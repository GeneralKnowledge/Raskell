fn calculate(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        let doubled = value * 2;
        if value > 0 {
            total += doubled;
        }
    }
    total
}
