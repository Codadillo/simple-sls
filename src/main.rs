use std::{error, fs::create_dir, io};

use clap::{arg, command, Parser};
use libc::pid_t;
use project::ptrace::Process;

/// SLSify compute-oriented applications
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// PID of the process to SLS
    #[arg(short, long)]
    pid: pid_t,

    /// Checkpoint directory path
    #[arg(short, long, default_value = "/tmp/slsdir")]
    cpath: String,
}

fn main() -> Result<(), Box<dyn error::Error>> {
    let Args { pid, cpath } = Args::parse();
    match create_dir(cpath) {
        Ok(_) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        e => e?,
    };

    let proc = Process::attach(pid)?;
    proc.wait()?;

    println!("{:?}", proc.get_regs()?);

    proc.resume()?;

    Ok(())
}
