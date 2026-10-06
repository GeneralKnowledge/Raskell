fn index_of(values: Vec<i32>, target: i32) -> Option<i32> {
    let mut i = 0;
    for v in values {
        if v == target {
            return Some(i);
        }
        i += 1;
    }
    None
}
