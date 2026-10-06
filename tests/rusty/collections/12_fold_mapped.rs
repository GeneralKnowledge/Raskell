fn sum_of_squares(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for x in values {
        total += x * x;
    }
    total
}
