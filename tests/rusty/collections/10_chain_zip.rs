fn combine(a: Vec<i32>, b: Vec<i32>) -> i32 {
    a.iter().chain(b.iter()).copied().sum()
}
fn pair_sum(a: Vec<i32>, b: Vec<i32>) -> Vec<i32> {
    a.iter().zip(b.iter()).map(|(x, y)| **x + **y).collect()
}
