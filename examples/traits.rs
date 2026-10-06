trait Greet {
    fn greet(&self) -> String;
}

struct Person {
    name: String,
}

impl Greet for Person {
    fn greet(&self) -> String {
        self.name.clone()
    }
}

impl Person {
    fn new(name: String) -> Person {
        Person { name }
    }

    fn name_len(&self) -> i32 {
        self.name.len() as i32
    }
}

fn say_hello<T: Greet>(x: T) -> String {
    x.greet()
}
