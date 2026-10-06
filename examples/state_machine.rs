enum State {
    Idle,
    Running,
    Done,
}

fn step(state: State) -> State {
    match state {
        State::Idle => State::Running,
        State::Running => State::Done,
        State::Done => State::Done,
    }
}

fn is_done(state: State) -> bool {
    match state {
        State::Done => true,
        _ => false,
    }
}
