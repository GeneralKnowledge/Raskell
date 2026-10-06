struct BoxVal {
    value: i32,
}
fn bump(b: BoxVal) -> BoxVal {
    BoxVal { value: b.value + 1 }
}
fn twice(b: BoxVal) -> BoxVal {
    bump(bump(b))
}
