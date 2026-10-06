use std::thread;
fn spawn_add() {
    thread::spawn(|| { let _ = 1 + 1; });
}
