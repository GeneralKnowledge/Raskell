fn process(values: Vec<i32>) -> i32 {
    let mut total = 0;
    let mut count = 0;
    for value in values {
        if value < 0 {
            continue;
        }
        let adjusted = value + 10;
        total += adjusted;
        count += 1;
    }
    if count == 0 {
        0
    } else {
        total / count
    }
}
