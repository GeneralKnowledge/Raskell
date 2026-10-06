enum Status {
    Idle,
    Running(i32),
    Done,
}
fn progress(s: Status) -> i32 {
    match s {
        Status::Idle => 0,
        Status::Running(n) => n,
        Status::Done => 100,
    }
}
