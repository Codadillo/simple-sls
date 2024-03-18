use std::{
    fs::File,
    io::{stdout, Read, Seek, Write},
    process::id,
    time::Duration,
};

fn main() {
    println!("{}", id());

    let mut file = File::open("README.md").unwrap();

    let mut buf = [0];
    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 {
            file.seek(std::io::SeekFrom::Start(0)).unwrap();
            continue;
        }

        print!("{}", String::from_utf8(buf.to_vec()).unwrap());
        stdout().lock().flush().unwrap();

        std::thread::sleep(Duration::from_secs_f64(0.5));
    }
}
