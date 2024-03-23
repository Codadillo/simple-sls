use std::{
    fs::read_dir,
    io::{stdin, BufRead},
};

use markov::Chain;

// This is gitignored out because it's massive.
const DIRECTORY: &str = "examples/dump";

const MAX_FILES: usize = 100;
const GEN_FROM_STDIN: bool = false;

fn main() {
    let training_data: Vec<_> = read_dir(DIRECTORY)
        .unwrap()
        .take(MAX_FILES)
        .map(|dir| dir.unwrap().path())
        .collect();

    let mut chain = Chain::new();

    for path in training_data {
        // technically assumes the file is
        // formatted a certain way, but idc
        chain.feed_file(path).unwrap();
    }

    if !GEN_FROM_STDIN {
        for s in chain.str_iter_for(100) {
            print!("{s} ");
        }

        println!("");
        return;
    }

    for line in stdin().lock().lines() {
        let len: usize = line.unwrap().parse().unwrap();

        for s in chain.str_iter_for(len) {
            print!("{s} ");
        }

        println!("");
    }
}
