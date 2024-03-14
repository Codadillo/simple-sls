use std::process::id;

use large_primes::lucas_lehmer_test;
use num_bigint::BigUint;

const UNBOUNDED: bool = true;

fn main() {
    println!("{}", id());

    let max_p: BigUint = 1000u32.into();

    let mut p: BigUint = 0u32.into();
    while UNBOUNDED || p < max_p {
        if lucas_lehmer_test(&p) {
            println!("2^{p} - 1 is prime");
        }

        p += 1u8;
    }
}
