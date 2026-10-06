use std::rc::Rc;
fn wrap(x: i32) -> Rc<i32> { Rc::new(x) }
