fn first_positive(values: &Vec<i32>) -> Option<i32> {
    for value in values {
        if *value > 0 {
            return Some(*value);
        }
    }
    None
}
