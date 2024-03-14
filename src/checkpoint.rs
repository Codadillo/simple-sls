use std::{
    error::Error,
    fs::{create_dir, hard_link, remove_dir_all, write, File, OpenOptions},
    io::{ErrorKind, Read, Seek, SeekFrom},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use libc::pid_t;
use log::{debug, info};
use procfs::process::{MMPermissions, MemoryMap, Process};

use crate::ptrace::{PTrace, Registers};

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

        let mut checkpointed_maps = vec![];
        for map in maps {
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

        Ok(VolatileCheckpoint {
            regs,
            maps: checkpointed_maps,
            mems,
            reusable_mems,
        })
    }

    pub fn checkpoint(&mut self) -> Result<(), Box<dyn Error>> {
        self.checkpoint_timed(false).map(|_| ())
    }

    pub fn checkpoint_timed(
        &mut self,
        time_pause: bool,
    ) -> Result<Option<Duration>, Box<dyn Error>> {
        self.step.seq = self.step.seq.wrapping_add(1);
        info!("Starting a checkpoint");

        let mut pause_time = None;
        let v_cp = if time_pause {
            let pause_start = Instant::now();
            let v_cp = self.volatile_checkpoint()?;
            pause_time = Some(pause_start.elapsed());

            v_cp
        } else {
            self.volatile_checkpoint()?
        };

        info!("Created in memory checkpoint");

        // Now that the process is resumed we can persist the checkpoint to disk
        let cp_dir = self.path.join(self.step.seq.to_string());
        info!("Checkpointing to {cp_dir:?}");

        maybe_remove_dir_all(&cp_dir)?;
        create_dir(&cp_dir)?;

        for (i, mem) in v_cp.mems {
            debug!("Writing maps[{i}]");

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

        serde_json::to_writer(File::create(cp_dir.join("regs"))?, &v_cp.regs)?;
        serde_json::to_writer(File::create(cp_dir.join("maps"))?, &v_cp.maps)?;

        self.step
            .seq_file
            .write_all_at(self.step.seq.to_string().as_bytes(), 0)?;
        self.step.last_maps = v_cp.maps;

        info!("Completed checkpoint");
        Ok(pause_time)
    }

    pub fn run(&mut self, period: Duration, max_cps: u64) -> Result<(), Box<dyn Error>> {
        let mut wait_time = period;
        loop {
            thread::sleep(wait_time);
            let start = Instant::now();

            self.checkpoint()?;
            self.cull_checkpoints(max_cps)?;

            wait_time = period.saturating_sub(start.elapsed());
        }
    }

    pub fn run_adaptive(
        &mut self,
        max_overhead: f64,
        min_period: Duration,
        max_cps: u64,
    ) -> Result<(), Box<dyn Error>> {
        assert!(max_overhead >= 0.);

        let mut wait_time = min_period;
        loop {
            thread::sleep(wait_time);

            let start = Instant::now();

            let paused_time = self.checkpoint_timed(true)?.unwrap().as_secs_f64();
            self.cull_checkpoints(max_cps)?;

            let cp_time = start.elapsed().as_secs_f64();

            // Calculate how long we should let the process run freely
            // so that this checkpoint added at most `max_overhead` percent
            // overhead to the program.
            //
            // max_overhead = paused_time / runtime
            // => runtime = paused_time / max_overhead
            let free_run_time = paused_time / max_overhead;
            let remaining_free_run_time = free_run_time - (cp_time - paused_time);
            let remaining_min_period = min_period.as_secs_f64() - cp_time;

            wait_time = Duration::from_secs_f64(remaining_free_run_time.max(remaining_min_period));
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
