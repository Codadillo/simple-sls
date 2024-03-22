use std::{
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use project::checkpoint::{maybe_remove_dir_all, Checkpointer};

const CP_DIR: &str = "cps";
const OUTPUT_DIR: &str = "out/overhead/";
const BIN: &str = "../target/release/examples/travelling_salesman";
const PERIOD: f64 = 1.;
const TRIALS: usize = 3;

fn run_trial(bin: &str, period: Duration, cp_stats: File) -> (Duration, Duration) {
    let start = Instant::now();
    let mut proc = Command::new(bin).spawn().unwrap();

    let mut cp = Checkpointer::attach(proc.id() as i32, CP_DIR.into()).unwrap();
    if let Err(e) = cp.run(period, 3, Some(cp_stats)) {
        println!("assuming process exited, {e:?}");
    }

    let res = proc.wait().unwrap();
    let cp_runtime = start.elapsed();

    println!("Process exited with {res} in {cp_runtime:?}");

    let start = Instant::now();
    let mut proc = Command::new(BIN).spawn().unwrap();
    let res = proc.wait().unwrap();
    let real_runtime = start.elapsed();

    println!("Real exited with {res} in {real_runtime:?}");

    println!(
        "Slowdown: {}",
        cp_runtime.as_secs_f64() / real_runtime.as_secs_f64()
    );

    (cp_runtime, real_runtime)
}

fn main() {
    let bin_path = PathBuf::from(BIN);
    let bin_name = bin_path.file_name().unwrap().to_str().unwrap();
    let output_dir = format!("{OUTPUT_DIR}/{bin_name}");

    maybe_remove_dir_all(CP_DIR).unwrap();
    maybe_remove_dir_all(&output_dir).unwrap();
    create_dir_all(CP_DIR).unwrap();
    create_dir_all(&output_dir).unwrap();

    let period = Duration::from_secs_f64(PERIOD);
    let output = PathBuf::from(output_dir);
    let mut stats = File::create(output.join("stats")).unwrap();
    
    let mut total_cp_time = Duration::from_secs(0);
    let mut total_real_time = Duration::from_secs(0);

    for trial in 0..TRIALS {
        println!("Starting trial {trial}...");

        let (cp, real) = run_trial(
            BIN,
            period,
            File::create(output.join(format!("cp_stats_{trial}"))).unwrap(),
        );

        writeln!(stats, "{},{}", cp.as_nanos(), real.as_nanos()).unwrap();

        total_cp_time += cp;
        total_real_time += real;
    }

    println!("Average slowdown: {}", total_cp_time.as_secs_f64() / total_real_time.as_secs_f64());
}
