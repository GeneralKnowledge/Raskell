fn identity<T>(x: T) -> T {
    x
}

fn wrap_option<T>(x: T) -> Option<T> {
    Some(x)
}

fn map_pair<A, B>(a: A, b: B) -> (A, B) {
    (a, b)
}

fn main() {
    let _ = identity(42);
    let _ = wrap_option(1);
    let _ = map_pair(1, true);
}
