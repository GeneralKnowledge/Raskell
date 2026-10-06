enum Shape {
    Circle { radius: i32 },
    Rect { width: i32, height: i32 },
}
fn area(s: Shape) -> i32 {
    match s {
        Shape::Circle { radius } => radius * radius * 3,
        Shape::Rect { width, height } => width * height,
    }
}
