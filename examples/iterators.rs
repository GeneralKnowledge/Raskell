fn double_all(values: Vec<i32>) -> Vec<i32> {
    values.iter().map(|x| *x * 2).collect()
}

fn filter_big(values: Vec<i32>) -> Vec<i32> {
    values
        .iter()
        .filter(|x| **x > 10)
        .map(|x| **x * 2)
        .collect()
}

fn main() {
    let xs = vec![1, 2, 3];
    let _ = double_all(xs);
}
