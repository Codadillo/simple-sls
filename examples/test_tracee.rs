use std::process::id;

fn main() {
    println!("{}", id());

    let mut a = 0u32;
    loop {
        a = a.wrapping_add(1);
    }
}
