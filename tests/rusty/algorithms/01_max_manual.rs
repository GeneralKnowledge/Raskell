fn max_of(values: Vec<i32>) -> i32 {
    let mut best = values[0];
    for v in values {
        if v > best {
            best = v;
        }
    }
    best
}
