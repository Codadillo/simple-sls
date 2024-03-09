use std::{
    error::Error,
    fs::{create_dir, write, File},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::Duration,
};

use libc::pid_t;
use log::{debug, info};
use procfs::process::{MMPermissions, MMapPath, MemoryMap, Process};

use crate::ptrace::PTrace;

pub struct Checkpointer {
    pub procfs: Process,
    // pub ptrace: PTrace,
    pub mem_file: File,

    pub period: Duration,
    pub path: PathBuf,

    pub seq: u64,
    pub last_maps: Vec<MemoryMap>,
}

impl Checkpointer {
    pub fn attach(pid: pid_t, period: Duration, path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let procfs = Process::new(pid)?;
        let mem_file = procfs.mem()?;

        Ok(Self {
            procfs,
            mem_file,

            period,
            path,

            seq: 0,
            last_maps: vec![],
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

                if immutable
                    && (matches!(map.pathname, MMapPath::Path(_)) || self.last_maps.contains(map))
                {
                    debug!(
                        "ignoring memory region {:x?} @ {:?} due to immutability",
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
                            "ignoring memory region {:x?} @ {:?} due to read error",
                            map.pathname, map.address
                        );
                        continue;
                    }
                    res => {
                        res?;
                    }
                };

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

        self.last_maps = maps.0;

        info!("Completed checkpoint");
        Ok(())
    }
}
