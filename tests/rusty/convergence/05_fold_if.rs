fn calculate(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        if value > 0 {
            total += value * 2;
        }
    }
    total
}
