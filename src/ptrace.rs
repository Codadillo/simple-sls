use std::{error, io, mem::MaybeUninit, ptr};

use libc::{
    c_int, kill, pid_t, ptrace, user_fpregs_struct, user_regs_struct, waitpid, PTRACE_ATTACH,
    PTRACE_DETACH, PTRACE_GETFPREGS, PTRACE_GETREGS
};
use serde::{Deserialize, Serialize};

use crate::compat::{UserFpregs, UserRegs};

#[derive(Debug)]
pub struct PTrace {
    /// The pid of the process, technically redundant with `procfs.pid`
    pub pid: pid_t,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registers {
    pub regs: UserRegs,
    pub fregs: UserFpregs,
}

impl PTrace {
    /// Attaches ptrace to the given `pid`
    ///
    /// Ptrace will detach from `pid` when the returned `Process` struct is dropped
    pub fn attach(pid: pid_t) -> Result<Self, Box<dyn error::Error>> {
        let res = unsafe {
            ptrace(
                PTRACE_ATTACH,
                pid,
                ptr::null() as *const (),
                ptr::null() as *const (),
            )
        };

        
        match res {
            0.. => Ok(Self { pid }),
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

    // /// Pauses the attached process until the return guard is dropped.
    // /// This function also waits for the process to be paused before returning.
    // pub fn pause_guard(&self) -> io::Result<PauseGuard> {
    //     self.stop()?;
    //     self.wait()?;
    //     Ok(PauseGuard { ptrace: &self })
    // }

    // /// Pauses the attached process
    // pub fn stop(&self) -> io::Result<()> {
    //     self.signal(SIGSTOP)
    // }

    // /// Resumes the attached process
    // pub fn resume(&self) -> io::Result<()> {
    //     self.signal(SIGCONT)
    // }

    /// Blocks until the attached process is paused
    pub fn wait(&self) -> io::Result<()> {
        let mut status = 0;
        let res = unsafe { waitpid(self.pid, &mut status as *mut _, 0) };

        if res != self.pid || !libc::WIFSTOPPED(status) {
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
}

impl Drop for PTrace {
    fn drop(&mut self) {
        unsafe {
            ptrace(
                PTRACE_DETACH,
                self.pid,
                ptr::null() as *const (),
                ptr::null() as *const (),
            );
        }
    }
}

// pub struct PauseGuard<'a> {
//     pub ptrace: &'a PTrace,
// }

// impl<'a> PauseGuard<'a> {
//     pub fn resume(self) -> io::Result<()> {
//         self.ptrace.resume()
//     }
// }

// impl<'a> Drop for PauseGuard<'a> {
//     fn drop(&mut self) {
//         self.ptrace.resume().unwrap();
//     }
// }
