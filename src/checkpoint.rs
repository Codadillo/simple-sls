use std::{
    error::Error,
    fs::{create_dir, hard_link, read_to_string, remove_dir_all, write, File, OpenOptions},
    io::{ErrorKind, Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use libc::pid_t;
use log::{debug, info};
use procfs::process::{FDInfo, MMPermissions, MMapPath, MemoryMap, Process};

use crate::{
    ptrace::{PTrace, Registers},
    restore,
};

// TODOS:
// - Threads (TLS, etc.)
// -- what do with vvar and vdso
// - File descriptors (basic)
// - more register sets (vectors)

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

        let mut seq_buf = String::new();
        seq_file.read_to_string(&mut seq_buf)?;
        let seq: u64 = seq_buf.parse().unwrap_or(0);

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

pub struct VolatileCheckpoint {
    pub regs: Registers,
    pub files: Vec<(FDInfo, u64)>,
    pub maps: Vec<MemoryMap>,
    pub mems: Vec<(usize, Vec<u8>)>,
    pub reusable_mems: Vec<(usize, usize)>,
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

    pub fn volatile_checkpoint(&mut self) -> Result<VolatileCheckpoint, Box<dyn Error>> {
        let maps = self.procfs.maps()?;
        let mut mems = vec![];
        let mut reusable_mems = vec![];
        let mut ptrace = PTrace::new(self.procfs.pid); // TODO: make this a member of self
        ptrace.attach()?;
        ptrace.wait_pause()?;
        info!("Attached ptrace");

        let regs = ptrace.get_regs()?;

        let mut files = vec![]; // I want try_collect
        for file in self.procfs.fd()? {
            let file = file?;

            // I feel like procfs should do this for me
            // Also because it doesn't I should probably make my own
            // fdinfo type struct so I don't have to have this vector of tuples
            let fdinfo_raw = read_to_string(format!("/proc/{}/fdinfo/{:?}", ptrace.pid, file.fd))?;
            let offset: u64 = fdinfo_raw
                .strip_prefix("pos:\t")
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .parse()?;

            files.push((file, offset));
        }

        let mut checkpointed_maps = vec![];
        for map in maps {
            // TODO: this is a way of avoiding checkpointing the bootstrapper's memory
            match map.pathname {
                MMapPath::Path(path)
                    if path
                        .to_str()
                        .filter(|p| p.contains(restore::BS_GUID))
                        .is_some() =>
                {
                    debug!("Ignoring bootstrapper memory mapping {path:?}");
                    continue;
                }
                _ => {}
            }

            let immutable = !map.perms.contains(MMPermissions::WRITE);

            if immutable {
                // It seems like there are some parts of an ELF file that will
                // end up in a read only memory mapping but differ from the on disk
                // version of the file, so for now we comment this out
                // if matches!(map.pathname, MMapPath::Path(_)) {
                //     debug!(
                //         "ignoring memory region {:?} @ {:x?}, it is an immutable file",
                //         map.pathname, map.address
                //     );
                //     continue;
                // }

                if let Some(old) = self
                    .step
                    .last_maps
                    .iter()
                    .enumerate()
                    .find_map(|(j, m)| (m == &map).then_some(j))
                {
                    let new = checkpointed_maps.len();
                    debug!(
                            "reusing old_maps[{old}] for memory region maps[{new}] = {:?}, it is immutable and already checkpointed",
                            map.pathname
                        );

                    reusable_mems.push((new, old));
                    checkpointed_maps.push(map);
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
                        "ignoring memory region map {:?} due to read error",
                        map.pathname
                    );
                    continue;
                }
                res => {
                    res?;
                }
            };

            let new = checkpointed_maps.len();
            debug!("saving memory region maps[{new}] = {:?}", map.pathname);
            mems.push((new, mem));
            checkpointed_maps.push(map);
        }

        // This is redundant because the process should 
        // get resumed when `ptrace` is dropped anyways.
        ptrace.resume()?;

        Ok(VolatileCheckpoint {
            regs,
            files,
            maps: checkpointed_maps,
            mems,
            reusable_mems,
        })
    }

    pub fn checkpoint(&mut self) -> Result<Duration, Box<dyn Error>> {
        self.step.seq = self.step.seq.wrapping_add(1);
        info!("Starting a checkpoint");

        let pause_start = Instant::now();
        let v_cp = self.volatile_checkpoint()?;
        let pause_time = pause_start.elapsed();

        info!("Created in memory checkpoint");

        // Now that the process is resumed we can persist the checkpoint to disk
        let cp_dir = self.path.join(self.step.seq.to_string());
        info!("Checkpointing to {cp_dir:?}");

        maybe_remove_dir_all(&cp_dir)?;
        create_dir(&cp_dir)?;

        for (i, mem) in v_cp.mems {
            debug!("Writing maps[{i}]");

            // syncing: this should fsync before it returns
            write(cp_dir.join(i.to_string()), mem)?;
        }

        for (new, old) in v_cp.reusable_mems {
            debug!("Linking maps[{new}] = old_maps[{old}]");

            hard_link(
                self.path
                    .join((self.step.seq - 1).to_string())
                    .join(old.to_string()),
                cp_dir.join(new.to_string()),
            )?;
        }

        // syncing: the scope ensures that the File structs are dropped at thus fsynced
        {
            serde_json::to_writer(File::create(cp_dir.join("regs"))?, &v_cp.regs)?;
            serde_json::to_writer(File::create(cp_dir.join("maps"))?, &v_cp.maps)?;
            serde_json::to_writer(File::create(cp_dir.join("files"))?, &v_cp.files)?;
        }

        self.step
            .seq_file
            .write_all_at(self.step.seq.to_string().as_bytes(), 0)?;

        // syncing: obviously this syncs the seq file,
        // and every other file write to our cp has already been fsynced
        self.step.seq_file.sync_all()?;

        self.step.last_maps = v_cp.maps;

        info!("Completed checkpoint");
        Ok(pause_time)
    }

    pub fn run(
        &mut self,
        period: Duration,
        max_cps: u64,
        mut stats: Option<impl Write>,
    ) -> Result<(), Box<dyn Error>> {
        let mut wait_time = period;
        loop {
            thread::sleep(wait_time);
            let start = Instant::now();

            let vcp_time = self.checkpoint()?;
            self.cull_checkpoints(max_cps)?;

            if let Some(stats) = &mut stats {
                let cp_time = start.elapsed();
                writeln!(stats, "{},{}", vcp_time.as_nanos(), cp_time.as_nanos())?;
            }

            wait_time = period.saturating_sub(start.elapsed());
        }
    }

    pub fn run_adaptive(
        &mut self,
        max_overhead: f64,
        min_period: Option<Duration>,
        max_period: Option<Duration>,
        max_cps: u64,
        mut stats: Option<impl Write>,
    ) -> Result<(), Box<dyn Error>> {
        assert!(max_overhead >= 0.);

        let mut wait_time = min_period.unwrap_or(max_period.unwrap_or(Duration::from_secs(0)));
        loop {
            thread::sleep(wait_time);

            let start = Instant::now();

            let paused_time = self.checkpoint()?;
            self.cull_checkpoints(max_cps)?;
            
            if let Some(stats) = &mut stats {
                let cp_time = start.elapsed();
                writeln!(stats, "{},{}", paused_time.as_nanos(), cp_time.as_nanos())?;
            }
            
            let cp_time = start.elapsed();

            // Calculate how long we should let the process run freely
            // so that this checkpoint added at most `max_overhead` percent
            // overhead to the program.
            //
            // max_overhead = paused_time / runtime
            // => runtime = paused_time / max_overhead
            let free_run_time = Duration::from_secs_f64(paused_time.as_secs_f64() / max_overhead);
            let remaining_free_run_time = free_run_time.saturating_sub(cp_time - paused_time);
            wait_time = remaining_free_run_time;

            if let Some(min_period) = min_period {
                wait_time = wait_time.max(min_period.saturating_sub(cp_time));
            }

            if let Some(max_period) = max_period {
                wait_time = wait_time.min(max_period.saturating_sub(cp_time));
            }

            info!("Waiting {wait_time:?} before next checkpoint (adaptive)");
        }
    }

    pub fn cull_checkpoints(&mut self, max_cps: u64) -> Result<(), Box<dyn Error>> {
        if self.step.seq < max_cps {
            return Ok(());
        }

        // FIXME: this won't work if self.step.seq wraps back around to 0
        self.clean_checkpoints(0..=self.step.seq - max_cps)
    }

    pub fn clean_checkpoints(
        &mut self,
        range: impl IntoIterator<Item = u64>,
    ) -> Result<(), Box<dyn Error>> {
        for cp in range {
            maybe_remove_dir_all(self.path.join(cp.to_string()))?
        }

        Ok(())
    }
}

pub fn maybe_remove_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
    match remove_dir_all(path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        e => e,
    }
}
