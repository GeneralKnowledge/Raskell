struct User {
    name: String,
    age: i32,
}
fn rename(mut user: User, name: String) -> User {
    user.name = name;
    user
}
