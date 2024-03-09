use std::{
    error::Error,
    fs::{write, File},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::Duration,
};

use libc::pid_t;
use procfs::process::{MMPermissions, MMapPath, MemoryMap, Process};

use crate::ptrace::PTrace;

pub struct Checkpointer {
    pub procfs: Process,
    pub ptrace: PTrace,
    pub mem_file: File,

    pub period: Duration,
    pub path: PathBuf,

    pub last_maps: Vec<MemoryMap>,
}

impl Checkpointer {
    pub fn attach(pid: pid_t, period: Duration, path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let procfs = Process::new(pid)?;
        let mem_file = procfs.mem()?;
        let ptrace = PTrace::attach(pid)?;
        ptrace.resume()?;

        Ok(Self {
            procfs,
            ptrace,
            mem_file,

            period,
            path,

            last_maps: vec![],
        })
    }

    pub fn checkpoint(&mut self) -> Result<(), Box<dyn Error>> {
        // Stop the process and get a checkpoint of its state
        self.ptrace.stop()?;

        let regs = self.ptrace.get_regs()?;
        let maps = self.procfs.maps()?;
        let mut mems = vec![];

        for map in &maps {
            let immutable = !map.perms.contains(MMPermissions::WRITE);

            if immutable
                && (matches!(map.pathname, MMapPath::Path(_)) || self.last_maps.contains(map))
            {
                continue;
            }

            let mut mem = vec![];
            (&self.mem_file).seek(SeekFrom::Start(map.address.0))?;
            (&self.mem_file)
                .take(map.address.1 - map.address.0)
                .read_to_end(&mut mem)?;
            mems.push(mem);
        }

        self.ptrace.resume()?;

        // Now that the process is resumed we can persist the checkpoint to disk
        serde_json::to_writer(File::create(self.path.join("regs"))?, &regs)?;
        serde_json::to_writer(File::create(self.path.join("maps"))?, &maps)?;

        for (i, mem) in mems.iter().enumerate() {
            write(self.path.join(i.to_string()), mem)?;
        }

        self.last_maps = maps.0;

        Ok(())
    }
}
