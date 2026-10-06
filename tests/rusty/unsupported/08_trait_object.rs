trait Draw { fn draw(&self); }
fn render(d: &dyn Draw) { d.draw(); }
