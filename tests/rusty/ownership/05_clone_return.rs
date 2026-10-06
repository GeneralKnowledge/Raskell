struct User {
    name: String,
}
fn name_of(user: &User) -> String {
    user.name.clone()
}
