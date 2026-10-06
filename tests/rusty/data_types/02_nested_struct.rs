struct Address {
    city: String,
}
struct Person {
    name: String,
    address: Address,
}
fn city_of(p: Person) -> String {
    p.address.city
}
