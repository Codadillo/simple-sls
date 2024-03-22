use std::{
    fs::read_dir,
    io::{stdin, BufRead},
    time::{Duration, Instant},
};

use markov::Chain;

// This is gitignored out because it's massive.
const DIRECTORY: &str = "examples/dump";

const MAX_TRAIN_TIME: Duration = Duration::from_secs(60 * 5); 
const GEN_FROM_STDIN: bool = false;

fn main() {
    let mut chain = Chain::new();

    let training_start = Instant::now();
    let dir = read_dir(DIRECTORY).unwrap();
    for dir_entry in dir {
        let path = dir_entry.unwrap().path();

        println!("Training chain with file {path:?}");
        let start = Instant::now();

        // technically assumes the file is
        // formatted a certain way, but idc
        chain.feed_file(path).unwrap();

        println!("Trained in {:?}", start.elapsed());

        if training_start.elapsed() >= MAX_TRAIN_TIME {
            break;
        }
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
