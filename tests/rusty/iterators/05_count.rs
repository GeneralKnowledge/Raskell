fn count_even(values: Vec<i32>) -> i32 {
    values.iter().filter(|x| **x % 2 == 0).count() as i32
}
