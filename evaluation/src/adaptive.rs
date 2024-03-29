use std::{
    fs::{create_dir_all, write, File},
    path::PathBuf,
    process::Command,
    time::Instant,
};

use project::checkpoint::{maybe_remove_dir_all, Checkpointer};

const CP_DIR: &str = "cps";
const BIN: &str = "../target/release/examples/travelling_salesman";
const OUTPUT_DIR: &str = "out/adaptive";

const OVERHEAD: f64 = 0.1;

fn main() {
    let bin_path = PathBuf::from(BIN);
    let bin_name = bin_path.file_name().unwrap().to_str().unwrap();
    let output_dir = format!("{OUTPUT_DIR}/{bin_name}_{OVERHEAD}");

    maybe_remove_dir_all(CP_DIR).unwrap();
    maybe_remove_dir_all(&output_dir).unwrap();
    create_dir_all(CP_DIR).unwrap();
    create_dir_all(&output_dir).unwrap();

    let cp_start = Instant::now();
    let mut proc = Command::new(BIN).spawn().unwrap();
    let pid = proc.id();

    let mut cp = Checkpointer::attach(pid as i32, CP_DIR.into()).unwrap();
    if let Err(e) = cp.run_adaptive(
        OVERHEAD,
        None,
        None,
        3,
        Some(File::create(format!("{output_dir}/cp_times")).unwrap()),
    ) {
        println!("assuming process exited, {e:?}");
    }

    let res = proc.wait().unwrap();
    let cp_elapsed = cp_start.elapsed();
    println!("checkpointed version exited with '{res}' in {cp_elapsed:?}");

    let real_start = Instant::now();
    let res = Command::new(BIN).spawn().unwrap().wait().unwrap();
    let real_elapsed = real_start.elapsed();
    println!("real version exited with '{res}' in {real_elapsed:?}");

    let cp_secs = cp_elapsed.as_secs_f64();
    let real_secs = real_elapsed.as_secs_f64();
    println!(
        "Measured overhead = {}, target overhead = {}",
        cp_secs / real_secs - 1.,
        OVERHEAD
    );

    write(
        format!("{output_dir}/overhead"),
        format!("{cp_secs},{real_secs}"),
    )
    .unwrap();
}
