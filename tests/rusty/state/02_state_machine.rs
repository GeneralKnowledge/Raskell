enum Phase {
    Start,
    Middle(i32),
    End,
}
fn step(p: Phase) -> Phase {
    match p {
        Phase::Start => Phase::Middle(1),
        Phase::Middle(n) => {
            if n >= 3 { Phase::End } else { Phase::Middle(n + 1) }
        }
        Phase::End => Phase::End,
    }
}
