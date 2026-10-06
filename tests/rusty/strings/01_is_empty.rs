fn label(name: String) -> String {
    if name.is_empty() { String::from("anonymous") } else { name }
}
