fn double_if_some(x: Option<i32>) -> Option<i32> {
    match x {
        Some(v) => Some(v * 2),
        None => None,
    }
}

fn main() {
    let a = double_if_some(Some(21));
    let b = double_if_some(None);
}
