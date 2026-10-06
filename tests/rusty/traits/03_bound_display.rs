trait Showable {
    fn show_it(&self) -> String;
}
fn display<T: Showable>(x: T) -> String { x.show_it() }
