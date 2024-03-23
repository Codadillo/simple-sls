use project::{
    checkpoint::{maybe_remove_dir_all, Checkpointer},
    restore::restore_checkpoint,
};
use rand::prelude::*;
use std::{
    fs::{create_dir_all, read_to_string, File},
    io::Write,
    ops::Range,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::Command,
    sync::{mpsc::channel, Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

const CP_DIR: &str = "cps";
const OUTPUT_DIR: &str = "out/stress";
const BIN: &str = "../target/release/examples/markov";
const KILL_TIME: Range<f64> = 0.5..1.;

fn main() {
    env_logger::init();

    let bin_path = PathBuf::from(BIN);
    let bin_name = bin_path.file_name().unwrap().to_str().unwrap();
    let output_dir = format!("{OUTPUT_DIR}/{bin_name}");

    maybe_remove_dir_all(CP_DIR).unwrap();
    maybe_remove_dir_all(&output_dir).unwrap();
    create_dir_all(CP_DIR).unwrap();
    create_dir_all(&output_dir).unwrap();

    let real_output_path = format!("{output_dir}/real");
    let test_output_path = format!("{output_dir}/test");

    let mut restore_output = File::create(format!("{output_dir}/restore")).unwrap();

    let mut real_proc = Command::new(BIN).arg(&real_output_path).spawn().unwrap();

    let mut proc = Command::new(BIN).arg(&test_output_path).spawn().unwrap();
    let pid = proc.id();

    let (p_send, p_recv) = channel();
    let (k_send, k_recv) = channel();
    let restore_b = Arc::new(Barrier::new(2));
    let restore_b_ = restore_b.clone();
    thread::spawn(move || {
        let mut pid = pid;
        loop {
            maybe_remove_dir_all(CP_DIR).unwrap();
            create_dir_all(CP_DIR).unwrap();

            let mut cp = Checkpointer::attach(pid as i32, CP_DIR.into()).unwrap();
            // cp.checkpoint().unwrap();
            k_send.send(()).unwrap();

            let r = cp.run(
                Duration::from_secs_f64(KILL_TIME.start) / 2,
                3,
                Option::<File>::None,
            );

            println!("[CP]: {r:?}");

            restore_b_.wait();

            let restore_start = Instant::now();
            let proc = match restore_checkpoint(&CP_DIR.into(), false) {
                Ok(p) => p,
                Err(e) => {
                    println!("[CP]: exiting ({e:?})");
                    return;
                }
            };
            let restore_time = restore_start.elapsed();
            writeln!(restore_output, "{},", restore_time.as_nanos()).unwrap();

            println!("[CP]: Restored {:?}", proc.id());

            pid = proc.id();
            p_send.send(proc).unwrap();
        }
    });

    let mut rng = thread_rng();
    let res = loop {
        thread::sleep(Duration::from_secs_f64(rng.gen_range(KILL_TIME)));

        k_recv.recv().unwrap();
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
