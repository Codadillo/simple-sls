use std::{error, fs::create_dir, io, time::Duration};

use clap::{arg, command, Parser};
use libc::pid_t;
use project::{checkpoint::Checkpointer, restore::restore_checkpoint};

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
        } => {
            match create_dir(&cpath) {
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                e => e?,
            };

            let mut cp = Checkpointer::attach(pid, cpath.clone().into())?;

            match period {
                Some(s) => cp.run(Duration::from_secs(s), max as u64)?,
                None => {
                    cp.checkpoint()?;
                    cp.cull_checkpoints(max as u64)?;
                }
            }
        }

        Args::Restore { cpath } => {
            restore_checkpoint(&cpath.into())?;
        }
    }

    Ok(())
}
