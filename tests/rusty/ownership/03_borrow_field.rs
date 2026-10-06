struct User {
    name: String,
    age: i32,
}
fn get_name(user: &User) -> &String {
    &user.name
}
