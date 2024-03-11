use std::{
    error::Error,
    fs::{create_dir, write, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{symlink, FileExt},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use libc::pid_t;
use log::{debug, info};
use procfs::process::{MMPermissions, MMapPath, MemoryMap, Process};

use crate::ptrace::PTrace;

// TODOS:
// - Threads (TLS, etc.)
// - File descriptors (basic)

pub struct StepData {
    pub seq: u64,
    pub seq_file: File,
    pub last_maps: Vec<MemoryMap>,
}

impl StepData {
    pub fn open(path: &PathBuf) -> Result<Self, Box<dyn Error>> {
        let mut seq_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path.join("seq"))?;

        let mut seq_buf = vec![];
        seq_file.read_to_end(&mut seq_buf)?;
        let seq = seq_buf
            .try_into()
            .map(|b| u64::from_le_bytes(b))
            .unwrap_or(0);

        let last_maps = if seq != 0 {
            let map_file = File::open(path.join(seq.to_string()).join("maps"))?;
            serde_json::from_reader(map_file)?
        } else {
            vec![]
        };

        Ok(Self {
            seq,
            seq_file,
            last_maps,
        })
    }
}

pub struct Checkpointer {
    pub procfs: Process,
    // pub ptrace: PTrace,
    pub mem_file: File,
    pub path: PathBuf,

    pub step: StepData,
}

impl Checkpointer {
    pub fn attach(pid: pid_t, path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let procfs = Process::new(pid)?;
        let mem_file = procfs.mem()?;

        Ok(Self {
            step: StepData::open(&path)?,

            procfs,
            mem_file,
            path,
        })
    }

    pub fn checkpoint(&mut self) -> Result<(), Box<dyn Error>> {
        self.step.seq += 1;
        info!("Starting a checkpoint");

        // Stop the process and get a checkpoint of its state
        let maps = self.procfs.maps()?;
        let mut mems = vec![];
        let mut reusable_mems = vec![];
        let regs = {
            let ptrace = PTrace::attach(self.procfs.pid)?;
            ptrace.wait()?;
            info!("Attached ptrace");

            let regs = ptrace.get_regs()?;

            for (i, map) in maps.iter().enumerate() {
                let immutable = !map.perms.contains(MMPermissions::WRITE);
                let is_file = matches!(map.pathname, MMapPath::Path(_));

                if immutable {
                    if is_file {
                        debug!(
                            "ignoring memory region {:?} @ {:x?}, it is an immutable file",
                            map.pathname, map.address
                        );
                        continue;
                    }

                    if let Some(old) = self
                        .step
                        .last_maps
                        .iter()
                        .enumerate()
                        .find_map(|(j, m)| (m == map).then_some(j))
                    {
                        debug!(
                            "ignoring memory region {:?} @ {:x?}, it is immutable and already checkpointed",
                            map.pathname, map.address
                        );

                        reusable_mems.push((i, old));
                        continue;
                    }
                }

                let mut mem = vec![];
                (&self.mem_file).seek(SeekFrom::Start(map.address.0))?;
                match (&self.mem_file)
                    .take(map.address.1 - map.address.0)
                    .read_to_end(&mut mem)
                {
                    Err(e) if e.raw_os_error() == Some(5) => {
                        debug!(
                            "ignoring memory region {:?} @ {:x?} due to read error",
                            map.pathname, map.address
                        );
                        continue;
                    }
                    res => {
                        res?;
                    }
                };

                debug!(
                    "saving memory rejoin {:?} @ {:x?}",
                    map.pathname, map.address
                );
                mems.push((i, mem));
            }

            regs
        };

        info!("Created in memory checkpoint");

        // Now that the process is resumed we can persist the checkpoint to disk
        let cp_dir = self.path.join(self.step.seq.to_string());
        create_dir(&cp_dir)?;

        serde_json::to_writer(File::create(cp_dir.join("regs"))?, &regs)?;
        serde_json::to_writer(File::create(cp_dir.join("maps"))?, &maps)?;

        for (i, mem) in mems {
            write(cp_dir.join(i.to_string()), mem)?;
        }

        for (new, old) in reusable_mems {
            // TODO: make this a hard link to make 
            // cleaning old checkpoints easier
            symlink(
                self.path
                    .join((self.step.seq - 1).to_string())
                    .join(old.to_string()),
                cp_dir.join(new.to_string()),
            )?;
        }

        self.step
            .seq_file
            .write_all_at(&self.step.seq.to_le_bytes(), 0)?;
        self.step.last_maps = maps.0;

        info!("Completed checkpoint");
        Ok(())
    }

    pub fn run(&mut self, period: Duration) -> Result<(), Box<dyn Error>> {
        let mut wait_time = period;
        loop {
            thread::sleep(wait_time);

            let start = Instant::now();
            self.checkpoint()?;
            wait_time = period.saturating_sub(start.elapsed());
        }
    }
}
