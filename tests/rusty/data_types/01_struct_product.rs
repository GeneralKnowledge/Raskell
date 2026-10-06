struct Product {
    name: String,
    price: i32,
    active: bool,
}
fn active_prices(products: Vec<Product>) -> Vec<i32> {
    products
        .iter()
        .filter(|p| p.active)
        .map(|p| p.price)
        .collect()
}
