struct User {
    name: String,
    age: u32,
}

fn greet(user: User) -> String {
    user.name
}

fn main() {
    let u = User {
        name: String::from("Ada"),
        age: 36,
    };
    let _ = greet(u);
}
