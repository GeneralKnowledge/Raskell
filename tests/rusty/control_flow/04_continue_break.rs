fn calculate(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        if value < 0 {
            continue;
        }
        total += value;
        if total > 100 {
            break;
        }
    }
    total
}
