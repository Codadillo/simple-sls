use std::{io, mem::MaybeUninit, ptr};

use libc::{
    c_int, kill, pid_t, ptrace, user_fpregs_struct, user_regs_struct, waitpid, PTRACE_ATTACH,
    PTRACE_DETACH, PTRACE_GETFPREGS, PTRACE_GETREGS, SIGCONT, SIGSTOP,
};

#[derive(Debug)]
pub struct Process {
    pub pid: pid_t,
}

#[derive(Debug, Clone)]
pub struct Registers {
    pub regs: user_regs_struct,
    pub fregs: user_fpregs_struct,
}

impl Process {
    pub fn attach(pid: pid_t) -> io::Result<Self> {
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
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn signal(&self, signal: c_int) -> io::Result<()> {
        let res = unsafe { kill(self.pid, signal) };

        match res {
            0.. => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn stop(&self) -> io::Result<()> {
        self.signal(SIGSTOP)
    }

    pub fn resume(&self) -> io::Result<()> {
        self.signal(SIGCONT)
    }

    pub fn wait(&self) -> io::Result<()> {
        let mut status = 0;
        let res = unsafe { waitpid(self.pid, &mut status as *mut _, 0) };

        if res != self.pid || !libc::WIFSTOPPED(status) {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    pub fn get_regs(&self) -> io::Result<Registers> {
        let mut regs = MaybeUninit::uninit();
        let mut fregs = MaybeUninit::uninit();

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
                regs: regs.assume_init(),
                fregs: fregs.assume_init(),
            })
        }
    }
}

impl Drop for Process {
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
