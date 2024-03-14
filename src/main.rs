use std::{
    error,
    fs::create_dir,
    io,
    time::Duration,
};

use clap::{arg, command, Parser};
use libc::pid_t;
use project::{
    checkpoint::{self, Checkpointer}, restore::restore_checkpoint
};

/// SLSify compute-oriented applications
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
enum Args {
    Checkpoint {
        /// PID of the process to SLS.
        #[arg(short, long)]
        pid: pid_t,

        /// Checkpoint period.
        /// If specified, rather than just checkpointing once,
        /// we will checkpoint once every period seconds.
        #[arg(short = 't', long)]
        period: Option<u64>,

        /// Checkpoint directory path.
        #[arg(short, long, default_value = "/tmp/slsdir")]
        cpath: String,

        /// The maximum number of checkpoints to keep on disk.
        #[arg(short, long, default_value = "3")]
        max: u32,

        /// Whether or not to delete the checkpoint directory first
        #[arg(short, long)]
        reset: bool,
    },

    Restore {
        /// Checkpoint directory path
        #[arg(short, long, default_value = "/tmp/slsdir")]
        cpath: String,
    },
}

fn main() -> Result<(), Box<dyn error::Error>> {
    env_logger::init();

    match Args::parse() {
        Args::Checkpoint {
            pid,
            period,
            cpath,
            max,
            reset,
        } => {
            if reset {
                checkpoint::maybe_remove_dir_all(&cpath)?;
            }

            match create_dir(&cpath) {
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                e => e?,
            };

            // let mut child = Command::new("target/debug/examples/travelling_salesman").spawn()?;
            // let mut child = Command::new("./a.out").spawn()?;
            // println!("{}", child.id());
            // unsafe { libc::kill(child.id() as _, libc::SIGSTOP) };

            // let mut p = PTrace::new(child.id() as _);
            // p.attach()?;
            // p.set_regs(p.get_regs()?)?;
            // p.detach()?;

            // let pid = child.id() as pid_t;
            let mut cp = Checkpointer::attach(pid, cpath.clone().into())?;

            match period {
                Some(s) => cp.run(Duration::from_secs(s), max as u64)?,
                None => {
                    cp.checkpoint()?;
                    cp.cull_checkpoints(max as u64)?;
                }
            }

            // child.wait()?;
        }

        Args::Restore { cpath } => {
            restore_checkpoint(&cpath.into())?;
        }
    }

    Ok(())
}
