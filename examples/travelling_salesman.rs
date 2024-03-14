use std::process;

use rand::{prelude::*, rngs::SmallRng};
use travelling_salesman::brute_force;

// with this set to 11, it takes me about 10 seconds to run
const TOWN_COUNT: usize = 11;

fn main() {
    println!("{}", process::id());

    // let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    // let secs = time.as_secs_f64();
    let mut rng = SmallRng::seed_from_u64(0);

    let towns: Vec<(f64, f64)> = (0..TOWN_COUNT).map(|_| (rng.gen(), rng.gen())).collect();
    let tour = brute_force::solve(&towns);

    println!("Finished: {:?}", tour.route);
}