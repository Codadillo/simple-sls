use libc::{c_uint, c_ulonglong, c_ushort, user_fpregs_struct, user_regs_struct};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRegs {
    pub r15: c_ulonglong,
    pub r14: c_ulonglong,
    pub r13: c_ulonglong,
    pub r12: c_ulonglong,
    pub rbp: c_ulonglong,
    pub rbx: c_ulonglong,
    pub r11: c_ulonglong,
    pub r10: c_ulonglong,
    pub r9: c_ulonglong,
    pub r8: c_ulonglong,
    pub rax: c_ulonglong,
    pub rcx: c_ulonglong,
    pub rdx: c_ulonglong,
    pub rsi: c_ulonglong,
    pub rdi: c_ulonglong,
    pub orig_rax: c_ulonglong,
    pub rip: c_ulonglong,
    pub cs: c_ulonglong,
    pub eflags: c_ulonglong,
    pub rsp: c_ulonglong,
    pub ss: c_ulonglong,
    pub fs_base: c_ulonglong,
    pub gs_base: c_ulonglong,
    pub ds: c_ulonglong,
    pub es: c_ulonglong,
    pub fs: c_ulonglong,
    pub gs: c_ulonglong,
}

impl From<user_regs_struct> for UserRegs {
    fn from(regs: user_regs_struct) -> Self {
        let user_regs_struct {
            r15,
            r14,
            r13,
            r12,
            rbp,
            rbx,
            r11,
            r10,
            r9,
            r8,
            rax,
            rcx,
            rdx,
            rsi,
            rdi,
            orig_rax,
            rip,
            cs,
            eflags,
            rsp,
            ss,
            fs_base,
            gs_base,
            ds,
            es,
            fs,
            gs,
        } = regs;

        Self {
            r15,
            r14,
            r13,
            r12,
            rbp,
            rbx,
            r11,
            r10,
            r9,
            r8,
            rax,
            rcx,
            rdx,
            rsi,
            rdi,
            orig_rax,
            rip,
            cs,
            eflags,
            rsp,
            ss,
            fs_base,
            gs_base,
            ds,
            es,
            fs,
            gs,
        }
    }
}

impl Into<user_regs_struct> for UserRegs {
    fn into(self) -> user_regs_struct {
        let Self {
            r15,
            r14,
            r13,
            r12,
            rbp,
            rbx,
            r11,
            r10,
            r9,
            r8,
            rax,
            rcx,
            rdx,
            rsi,
            rdi,
            orig_rax,
            rip,
            cs,
            eflags,
            rsp,
            ss,
            fs_base,
            gs_base,
            ds,
            es,
            fs,
            gs,
        } = self;
        user_regs_struct {
            r15,
            r14,
            r13,
            r12,
            rbp,
            rbx,
            r11,
            r10,
            r9,
            r8,
            rax,
            rcx,
            rdx,
            rsi,
            rdi,
            orig_rax,
            rip,
            cs,
            eflags,
            rsp,
            ss,
            fs_base,
            gs_base,
            ds,
            es,
            fs,
            gs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFpregs {
    pub cwd: c_ushort,
    pub swd: c_ushort,
    pub ftw: c_ushort,
    pub fop: c_ushort,
    pub rip: c_ulonglong,
    pub rdp: c_ulonglong,
    pub mxcsr: c_uint,
    pub mxcr_mask: c_uint,
    pub st_space: [c_uint; 32],
    pub xmm_space: Vec<c_uint>,
}

impl From<user_fpregs_struct> for UserFpregs {
    fn from(fregs: user_fpregs_struct) -> Self {
        let user_fpregs_struct {
            cwd,
            swd,
            ftw,
            fop,
            rip,
            rdp,
            mxcsr,
            mxcr_mask,
            st_space,
            xmm_space,
            ..
        } = fregs;
        Self {
            cwd,
            swd,
            ftw,
            fop,
            rip,
            rdp,
            mxcsr,
            mxcr_mask,
            st_space,
            xmm_space: xmm_space.to_vec(),
        }
    }
}

impl UserFpregs {
    pub fn into(self, buf: &mut user_fpregs_struct) {
        buf.cwd = self.cwd;
        buf.swd = self.swd;
        buf.ftw = self.ftw;
        buf.fop = self.fop;
        buf.rip = self.rip;
        buf.rdp = self.rdp;
        buf.mxcsr = self.mxcsr;
        buf.mxcr_mask = self.mxcr_mask;
        buf.st_space = self.st_space;
        buf.xmm_space = self.xmm_space.try_into().unwrap();
    }
}
