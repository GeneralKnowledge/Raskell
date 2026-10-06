trait Greet {
    fn greet(&self) -> String;
}
struct Person { name: String }
impl Greet for Person {
    fn greet(&self) -> String { self.name.clone() }
}
fn say_hello<T: Greet>(x: T) -> String { x.greet() }
