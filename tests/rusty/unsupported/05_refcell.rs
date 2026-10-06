use std::cell::RefCell;
fn bump(c: &RefCell<i32>) {
    *c.borrow_mut() += 1;
}
