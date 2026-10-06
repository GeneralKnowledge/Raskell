fn score(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        let bumped = value + 1;
        let scaled = bumped * 3;
        if scaled > 10 {
            total += scaled;
        }
    }
    total
}
