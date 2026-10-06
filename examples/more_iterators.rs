fn combine(a: Vec<i32>, b: Vec<i32>) -> i32 {
    a.iter().chain(b.iter()).copied().sum()
}

fn take_three(values: Vec<i32>) -> Vec<i32> {
    values.iter().take(3).copied().collect()
}
