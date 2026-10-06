fn sum_and_count(values: Vec<i32>) -> (i32, i32) {
    let mut sum = 0;
    let mut count = 0;
    for v in values {
        sum += v;
        count += 1;
    }
    (sum, count)
}
