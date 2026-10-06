fn triangle(n: i32) -> i32 {
    let mut i = n;
    let mut total = 0;
    while i > 0 {
        total += i;
        i -= 1;
    }
    total
}

fn main() {
    let _ = triangle(5);
}
