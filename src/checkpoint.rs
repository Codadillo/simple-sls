use std::{
    error::Error,
    fs::{create_dir, write, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::FileExt,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use libc::pid_t;
use log::{debug, info};
use procfs::process::{MMPermissions, MMapPath, MemoryMap, Process};

use crate::ptrace::PTrace;

pub struct Checkpointer {
    pub procfs: Process,
    // pub ptrace: PTrace,
    pub mem_file: File,
    pub path: PathBuf,

    pub seq: u64,
    pub seq_file: File,
    pub last_maps: Vec<MemoryMap>,
}

impl Checkpointer {
    pub fn attach(pid: pid_t, path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let procfs = Process::new(pid)?;
        let mem_file = procfs.mem()?;

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
            procfs,
            mem_file,
            path,

            seq,
            seq_file,
            last_maps,
        })
    }

    pub fn checkpoint(&mut self) -> Result<(), Box<dyn Error>> {
        self.seq += 1;
        info!("Starting a checkpoint");

        // Stop the process and get a checkpoint of its state
        let (regs, maps, mems) = {
            let ptrace = PTrace::attach(self.procfs.pid)?;
            ptrace.wait()?;
            info!("Attached ptrace");

            let regs = ptrace.get_regs()?;
            let maps = self.procfs.maps()?;
            let mut mems = vec![];

            for (i, map) in maps.iter().enumerate() {
                let immutable = !map.perms.contains(MMPermissions::WRITE);
                let is_file = matches!(map.pathname, MMapPath::Path(_));

                if immutable && (is_file || self.last_maps.contains(map)) {
                    debug!(
                        "ignoring memory region {:?} @ {:x?} due to immutability () (is_file = {is_file})",
                        map.pathname, map.address
                    );
                    continue;
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

            (regs, maps, mems)
        };

        info!("Created in memory checkpoint");

        // Now that the process is resumed we can persist the checkpoint to disk
        let cp_dir = self.path.join(self.seq.to_string());
        create_dir(&cp_dir)?;

        serde_json::to_writer(File::create(cp_dir.join("regs"))?, &regs)?;
        serde_json::to_writer(File::create(cp_dir.join("maps"))?, &maps)?;

        for (i, mem) in mems {
            write(cp_dir.join(i.to_string()), mem)?;
        }

        self.seq_file.write_all_at(&self.seq.to_le_bytes(), 0)?;
        self.last_maps = maps.0;

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
