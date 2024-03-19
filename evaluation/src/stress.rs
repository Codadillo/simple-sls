use project::{
    checkpoint::{maybe_remove_dir_all, Checkpointer},
    restore::restore_checkpoint,
};
use rand::prelude::*;
use std::{
    fs::{create_dir_all, read_to_string, File},
    ops::Range,
    os::unix::process::ExitStatusExt,
    process::Command,
    sync::{mpsc::channel, Arc, Barrier},
    thread,
    time::Duration,
};

const CP_DIR: &str = "cps";
const OUTPUT_DIR: &str = "out/stress";
const BIN: &str = "../target/debug/examples/travelling_salesman";
const KILL_TIME: Range<f64> = 0.5..1.;

fn main() {
    env_logger::init();

    maybe_remove_dir_all(CP_DIR).unwrap();
    maybe_remove_dir_all(OUTPUT_DIR).unwrap();
    create_dir_all(CP_DIR).unwrap();
    create_dir_all(OUTPUT_DIR).unwrap();

    let real_output_path = format!("{OUTPUT_DIR}/real");
    let test_output_path = format!("{OUTPUT_DIR}/test");

    let mut real_proc = Command::new(BIN).arg(&real_output_path).spawn().unwrap();

    let mut proc = Command::new(BIN).arg(&test_output_path).spawn().unwrap();
    let pid = proc.id();

    let (p_send, p_recv) = channel();
    let restore_b = Arc::new(Barrier::new(2));
    let restore_b_ = restore_b.clone();
    thread::spawn(move || {
        let mut pid = pid;
        loop {
            let mut cp = Checkpointer::attach(pid as i32, CP_DIR.into()).unwrap();
            let r = cp.run(
                Duration::from_secs_f64(KILL_TIME.start) / 2,
                3,
                Option::<File>::None,
            );

            println!("[CP]: {r:?}");

            restore_b_.wait();

            let proc = match restore_checkpoint(&CP_DIR.into(), false) {
                Ok(p) => p,
                Err(e) => {
                    println!("[CP]: exiting ({e:?})");
                    return;
                }
            };

            pid = proc.id();
            p_send.send(proc).unwrap();
        }
    });

    let mut rng = thread_rng();
    let res = loop {
        thread::sleep(Duration::from_secs_f64(rng.gen_range(KILL_TIME)));

        println!("[M]: Killing {}", proc.id());
        proc.kill().unwrap();

        let res = proc.wait().unwrap();
        if res.into_raw() != 9 {
            println!("[M]: {} completed: {res:?}", proc.id());
            break res;
        }

        restore_b.wait();
        proc = match p_recv.recv() {
            Ok(proc) => proc,
            Err(e) => {
                println!("[M]: done {e:?}");
                break proc.wait().unwrap();
            }
        };
    };

    println!("[M]: test completed with {res}");
    let real_res = real_proc.wait().unwrap();
    println!("[M]: real completed with {real_res}");

    let test_output = read_to_string(test_output_path).unwrap();
    let real_output = read_to_string(real_output_path).unwrap();
    println!("[M] test output: {test_output}");
    println!("[M] real_output: {real_output}");

    assert_eq!(res, real_res);
    assert_eq!(test_output, real_output);
}