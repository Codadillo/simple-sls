use std::{
    error,
    fs::{create_dir, File},
    io::{self, Write},
    time::Duration,
};

use clap::{arg, command, Parser};
use libc::pid_t;
use project::{
    checkpoint::{self, Checkpointer},
    restore::restore_checkpoint,
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
        period: Option<f64>,

        /// Runs checkpointing with an adaptive period
        /// such that it will never impose more than `overhead`` percent
        /// overhead on the checkpointed program.
        ///
        /// If `period` is not specified, the minimum period will be
        /// 1 second, otherwise `period` is used as the minimum period.
        #[arg(short, long)]
        overhead: Option<f64>,

        /// Checkpoint directory path.
        #[arg(short, long, default_value = "/tmp/slsdir")]
        cpath: String,

        /// The maximum number of checkpoints to keep on disk.
        #[arg(short, long, default_value = "3")]
        max: u32,

        /// Whether or not to delete the checkpoint directory first
        #[arg(short, long)]
        reset: bool,

        /// A path to store checkpointing statistics.
        #[arg(short, long)]
        stats: Option<String>,
    },

    Restore {
        /// Checkpoint directory path
        #[arg(short, long, default_value = "/tmp/slsdir")]
        cpath: String,

        /// If specified, the restored program's pid will be printed and
        /// it will remain in a SIGSTOPed state so you can, for example,
        /// attach gdb to it and debug the restoration.
        #[arg(short, long)]
        hang: bool,
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
            overhead,
            stats,
        } => {
            if reset {
                checkpoint::maybe_remove_dir_all(&cpath)?;
            }

            match create_dir(&cpath) {
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                e => e?,
            };

            let stats = match stats {
                Some(path) => Some(File::create(path)?),
                None => None,
            };

            let mut cp = Checkpointer::attach(pid, cpath.clone().into())?;

            if let Some(overhead) = overhead {
                cp.run_adaptive(
                    overhead,
                    Duration::from_secs_f64(period.unwrap_or(1.)),
                    max as u64,
                    stats,
                )?;

                return Ok(());
            }

            match period {
                Some(s) => cp.run(Duration::from_secs_f64(s), max as u64, stats)?,
                None => {
                    let vcp_time = cp.checkpoint()?;
                    cp.cull_checkpoints(max as u64)?;

                    if let Some(mut stats) = stats {
                        write!(stats, "{}", vcp_time.as_nanos())?;
                    }
                }
            }
        }

        Args::Restore { cpath, hang } => {
            restore_checkpoint(&cpath.into(), hang)?;
        }
    }

    Ok(())
}
