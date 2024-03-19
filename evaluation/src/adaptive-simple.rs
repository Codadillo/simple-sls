use std::{
    error::Error,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use project::ptrace::PTrace;

const BIN: &str = "../target/release/examples/travelling_salesman";
const OVERHEAD: f64 = 0.1;
const PAUSE_TIME: f64 = 0.5;

fn main() {
    let start = Instant::now();
    let mut proc = Command::new(BIN).spawn().unwrap();
    let pid = proc.id();

    let timer = thread::spawn(move || {
        let res = proc.wait().unwrap();
        let out = start.elapsed();
        println!("[T]: Process finished with {res}");

        out
    });

    let mut total_pausing = 0.;
    loop {
        if let Err(e) = fake_checkpoint(pid as i32, PAUSE_TIME) {
            println!("[M]: Assuming process finished at {e:?}");
            break;
        }

        total_pausing += PAUSE_TIME;
        thread::sleep(Duration::from_secs_f64(PAUSE_TIME / OVERHEAD));
    }

    let test_elapsed = timer.join().unwrap();

    let mut proc = Command::new(BIN).spawn().unwrap();
    let start = Instant::now();
    proc.wait().unwrap();
    let real_elapsed = start.elapsed();

    println!("Test time = {test_elapsed:?}, real time = {real_elapsed:?}, total pausing = {total_pausing}s");
    println!(
        "Measured overhead = {}, target overhead = {OVERHEAD}",
        test_elapsed.as_secs_f64() / real_elapsed.as_secs_f64() - 1.
    );
}

fn fake_checkpoint(pid: i32, pause_time: f64) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let mut ptrace = PTrace::new(pid);
    ptrace.attach()?;
    ptrace.wait_pause()?;

    thread::sleep(Duration::from_secs_f64(pause_time).saturating_sub(start.elapsed()));

    ptrace.resume()?;

    Ok(())
}
