fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn pow(base: i32, exp: i32) -> i32 {
    if exp <= 0 {
        1
    } else {
        base * pow(base, exp - 1)
    }
}
