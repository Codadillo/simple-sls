use std::{fs::read_dir, time::Instant};

use markov::Chain;

// This is gitignored out because it's massive.
const DIRECTORY: &str = "examples/dump";

fn main() {
    let mut chain = Chain::new();

    let dir = read_dir(DIRECTORY).unwrap();
    for dir_entry in dir {
        let path = dir_entry.unwrap().path();

        println!("Training chain with file {path:?}");
        let start = Instant::now();

        // technically assumes the file is
        // formatted a certain way, but idc
        chain.feed_file(path).unwrap();

        println!("Trained in {:?}", start.elapsed())
    }

    for s in chain.str_iter_for(100) {
        print!("{s} ");
    }

    println!("");
}
