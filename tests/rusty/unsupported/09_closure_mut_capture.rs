fn make_counter() -> impl FnMut() -> i32 {
    let mut n = 0;
    move || { n += 1; n }
}
