enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}

fn describe(shape: Shape) -> f64 {
    match shape {
        Shape::Circle(r) => r * r,
        Shape::Rectangle(w, h) => w * h,
    }
}

fn abs_value(x: i32) -> i32 {
    match x > 0 {
        true => x,
        false => -x,
    }
}
