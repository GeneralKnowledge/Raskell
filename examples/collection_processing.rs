fn double_push(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for value in values {
        result.push(value * 2);
    }
    result
}

fn sum_all(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        total += value;
    }
    total
}
