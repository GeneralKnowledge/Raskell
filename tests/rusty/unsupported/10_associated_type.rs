trait Container {
    type Item;
    fn first(&self) -> Option<&Self::Item>;
}
