fn scale(x: i32) -> i32 {
    let mut y = x;
    y += 5;
    y *= 2;
    y
}

fn sum_positive(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        if value > 0 {
            total += value;
        }
    }
    total
}

fn main() {
    let _ = scale(10);
    let _ = sum_positive(vec![1, -2, 3, -4, 5]);
}
