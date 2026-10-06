fn is_sorted(values: Vec<i32>) -> bool {
    let mut prev = values[0];
    for v in values {
        if v < prev {
            return false;
        }
        prev = v;
    }
    true
}
