use std::{error, io, mem::MaybeUninit, ptr};

use libc::{
    c_int, kill, pid_t, ptrace, user_fpregs_struct, user_regs_struct, waitpid, PTRACE_ATTACH,
    PTRACE_DETACH, PTRACE_GETFPREGS, PTRACE_GETREGS, PTRACE_SETFPREGS, PTRACE_SETREGS, SIGCONT,
    SIGSTOP, WUNTRACED,
};
use serde::{Deserialize, Serialize};

use crate::compat::{UserFpregs, UserRegs};

#[derive(Debug)]
pub struct PTrace {
    /// The pid of the process, technically redundant with `procfs.pid`
    pub pid: pid_t,
    /// Whether or not we are attached to the process
    pub attached: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registers {
    pub regs: UserRegs,
    pub fregs: UserFpregs,
}

impl PTrace {
    /// Creates a new PTrace, but does not actually
    /// start ptracing or anything
    pub fn new(pid: pid_t) -> Self {
        Self {
            pid,
            attached: false,
        }
    }

    /// Attaches ptrace to the given `pid`
    ///
    /// Ptrace will detach from `pid` when the returned `Process` struct is dropped
    pub fn attach(&mut self) -> Result<(), Box<dyn error::Error>> {
        let res = unsafe {
            ptrace(
                PTRACE_ATTACH,
                self.pid,
                ptr::null() as *const (),
                ptr::null() as *const (),
            )
        };

        match res {
            0.. => {
                self.attached = true;
                Ok(())
            }
            _ => Err(io::Error::last_os_error().into()),
        }
    }

    pub fn detach(&mut self) -> Result<(), Box<dyn error::Error>> {
        let res = unsafe {
            ptrace(
                PTRACE_DETACH,
                self.pid,
                ptr::null() as *const (),
                ptr::null() as *const (),
            )
        };

        match res {
            0.. => {
                self.attached = true;
                Ok(())
            }
            _ => Err(io::Error::last_os_error().into()),
        }
    }

    /// Raises `signal` in the attached process
    pub fn signal(&self, signal: c_int) -> io::Result<()> {
        let res = unsafe { kill(self.pid, signal) };

        match res {
            0.. => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// Pauses the attached process until the return guard is dropped.
    /// This function also waits for the process to be paused before returning.
    pub fn pause_guard(&self) -> io::Result<PauseGuard> {
        self.stop()?;
        self.wait_pause()?;
        Ok(PauseGuard { ptrace: &self })
    }

    /// Pauses the attached process
    pub fn stop(&self) -> io::Result<()> {
        self.signal(SIGSTOP)
    }

    /// Resumes the attached process
    pub fn resume(&self) -> io::Result<()> {
        self.signal(SIGCONT)
    }

    /// Wait until the process raises a signal
    pub fn wait(&self, options: i32) -> io::Result<i32> {
        let mut status = 0;
        let res = unsafe { waitpid(self.pid, &mut status as *mut _, options) };

        if res != self.pid {
            return Err(io::Error::last_os_error());
        }

        Ok(status)
    }

    /// Blocks until the attached process is paused while being traced by ptrace
    pub fn wait_pause(&self) -> io::Result<()> {
        self.wait_pause_inner(0)
    }

    /// Blocks until the process is paused in general
    pub fn wait_pause_unattached(&self) -> io::Result<()> {
        self.wait_pause_inner(WUNTRACED)
    }

    fn wait_pause_inner(&self, options: i32) -> io::Result<()> {
        let status = self.wait(options)?;

        if !libc::WIFSTOPPED(status) {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    /// Reads the register files of the attached process
    ///
    /// The process should be paused (e.g. with `Self::Stop`) when calling this
    pub fn get_regs(&self) -> io::Result<Registers> {
        let mut regs: MaybeUninit<user_regs_struct> = MaybeUninit::uninit();
        let mut fregs: MaybeUninit<user_fpregs_struct> = MaybeUninit::uninit();

        let res = unsafe {
            ptrace(
                PTRACE_GETREGS,
                self.pid,
                ptr::null() as *const (),
                regs.as_mut_ptr(),
            )
        };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        let res = unsafe {
            ptrace(
                PTRACE_GETFPREGS,
                self.pid,
                ptr::null() as *const (),
                fregs.as_mut_ptr(),
            )
        };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        unsafe {
            Ok(Registers {
                regs: regs.assume_init().into(),
                fregs: fregs.assume_init().into(),
            })
        }
    }

    /// Sets the register files of the attached process
    ///
    /// The process should be paused (e.g. with `Self::Stop`) when calling this
    pub fn set_regs(&self, Registers { regs, fregs }: Registers) -> io::Result<()> {
        let res = unsafe { ptrace(PTRACE_SETREGS, self.pid, ptr::null() as *const (), &regs) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        let res = unsafe { ptrace(PTRACE_SETFPREGS, self.pid, ptr::null() as *const (), &fregs) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }
}

impl Drop for PTrace {
    fn drop(&mut self) {
        if !self.attached {
            return;
        }

        let _ = self.detach();
    }
}

pub struct PauseGuard<'a> {
    pub ptrace: &'a PTrace,
}

impl<'a> PauseGuard<'a> {
    pub fn resume(self) -> io::Result<()> {
        self.ptrace.resume()
    }
}

impl<'a> Drop for PauseGuard<'a> {
    fn drop(&mut self) {
        self.ptrace.resume().unwrap();
    }
}
