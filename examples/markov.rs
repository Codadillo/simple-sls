use std::{
    fs::{metadata, read_dir},
    io::{stdin, BufRead},
    os::unix::fs::MetadataExt,
};

use markov::Chain;

// This is gitignored out because it's massive.
const DIRECTORY: &str = "../examples/dump";

const NUM_FILES: usize = 10_000;
const GEN_FROM_STDIN: bool = false;

fn main() {
    let training_data: Vec<_> = read_dir(DIRECTORY)
        .unwrap()
        .take(NUM_FILES)
        .filter_map(|dir| {
            let path = dir.unwrap().path();
            (metadata(&path).unwrap().size() < 20_000).then(|| path)
        })
        .collect();

    println!("{}", training_data.len());

    let mut chain = Chain::new();

    for path in std::iter::repeat(&training_data).flatten().take(NUM_FILES) {
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
