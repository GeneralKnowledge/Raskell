enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}

fn area(shape: Shape) -> f64 {
    match shape {
        Shape::Circle(r) => r * r,
        Shape::Rectangle(w, h) => w * h,
    }
}

fn main() {
    let a = area(Shape::Circle(2.0));
    let b = area(Shape::Rectangle(3.0, 4.0));
    println!("{}", a);
    println!("{}", b);
}
