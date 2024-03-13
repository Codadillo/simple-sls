use std::{io::{stdout, Write}, process::id};

fn main() {
    println!("pid: {}", id());

    let mut a = 0u32;
    let mut b = 0u32;
    loop {
        a = a.wrapping_add(1);

        if a % (u32::MAX / 4) == 0 {
            b = b.wrapping_add(1);
    
            print!("{b} ");
            stdout().flush().unwrap();
        }
    }
}
