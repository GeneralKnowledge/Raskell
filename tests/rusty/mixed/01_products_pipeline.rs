struct Product {
    name: String,
    price: i32,
    active: bool,
}
fn total_active(products: Vec<Product>) -> i32 {
    products
        .iter()
        .filter(|p| p.active)
        .map(|p| p.price)
        .sum()
}
fn expensive(products: Vec<Product>, min: i32) -> Vec<String> {
    let mut names = Vec::new();
    for p in products {
        if p.active && p.price >= min {
            names.push(p.name);
        }
    }
    names
}
