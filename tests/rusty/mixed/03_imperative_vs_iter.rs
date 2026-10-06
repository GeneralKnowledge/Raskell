fn via_iter(values: Vec<i32>) -> Vec<i32> {
    values.iter().filter(|x| **x > 0).map(|x| **x * 2).collect()
}
fn via_loop(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for x in values {
        if x > 0 {
            result.push(x * 2);
        }
    }
    result
}
