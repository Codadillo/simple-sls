use std::{
    error::Error,
    ffi::CString,
    fs::{File, Permissions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use goblin::{
    container::{Container, Ctx, Endian},
    elf::{
        header::{header64, EM_X86_64, ET_EXEC},
        program_header::{program_header64, PF_R, PF_W, PF_X},
        Header, ProgramHeader,
    },
};
use libc::{
    exit, pid_t, SYS_close, SYS_getpid, SYS_kill, SYS_mmap, SYS_munmap, SYS_open, MAP_FIXED,
    MAP_PRIVATE, O_RDONLY, SIGSTOP, S_IRGRP, S_IRUSR, S_IWUSR, S_IXGRP, S_IXUSR,
};
use log::info;
use procfs::process::MemoryMap;
use scroll::Pwrite;

use crate::{
    checkpoint::StepData,
    ptrace::{PTrace, Registers},
};

// TODO: more portability, this whole thing is pretty messy

pub fn create_bootstrapper(
    output_path: impl AsRef<Path>,
    checkpoint_dir: &PathBuf,
    maps: Vec<MemoryMap>,
) -> Result<(), Box<dyn Error>> {
    let vaddr = 0xe0000;
    let data_addr = vaddr + header64::SIZEOF_EHDR as u64 + program_header64::SIZEOF_PHDR as u64;

    let (data, program) = assemble_bs_code(checkpoint_dir, maps, vaddr, data_addr)?;

    write_bs_elf(output_path, vaddr, data, program)
}

pub fn write_bs_elf(
    output_path: impl AsRef<Path>,
    vaddr: u64,
    data: Vec<u8>,
    program: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    // https://github.com/tchajed/minimal-elf/

    let header_size = header64::SIZEOF_EHDR as u64;
    let pheader_size = program_header64::SIZEOF_PHDR as u64;
    let program_offset = header_size + pheader_size + data.len() as u64;
    let entry = vaddr + program_offset;

    let header: header64::Header = Header {
        e_type: ET_EXEC,
        e_machine: EM_X86_64,
        e_entry: entry,
        e_phoff: header_size as u64,
        e_phnum: 1,

        ..Header::new(Ctx::new(Container::Big, Endian::Little))
    }
    .into();

    let pheader: program_header64::ProgramHeader = ProgramHeader {
        p_flags: PF_R | PF_W | PF_X,
        p_offset: program_offset,
        p_vaddr: entry,
        p_filesz: program.len() as u64,
        p_memsz: program.len() as u64,

        ..ProgramHeader::new()
    }
    .into();

    let mut headers = vec![0u8; (header_size + pheader_size) as usize];
    headers.pwrite(header, 0).unwrap();
    headers.pwrite(pheader, header_size as usize).unwrap();

    let mut outfile = File::create(output_path)?;
    outfile.write_all(&headers)?;
    outfile.write_all(&data)?;
    outfile.write_all(&program)?;

    let perms = Permissions::from_mode(S_IRUSR | S_IWUSR | S_IXUSR | S_IRGRP | S_IXGRP);
    outfile.set_permissions(perms)?;

    Ok(())
}

/// Returns (data, program)
pub fn assemble_bs_code(
    cp_dir: &PathBuf,
    maps: Vec<MemoryMap>,
    vaddr: u64,
    data_addr: u64,
) -> Result<(Vec<u8>, Vec<u8>), Box<dyn Error>> {
    let mut data: Vec<u8> = vec![];
    let mut mmap_args = vec![];
    for (i, map) in maps.into_iter().enumerate() {
        let addr = map.address.0;
        let len = map.address.1 - addr;
        let prot = map.perms.bits();

        let flags = MAP_FIXED | MAP_PRIVATE;
        let (file_path, offset) = match map.pathname {
            // It seems like there are some parts of an ELF file that will
            // end up in a read only memory mapping but differ from the on disk
            // version of the file, so for now we comment this out
            // MMapPath::Path(path) if !map.perms.contains(MMPermissions::WRITE) => (path, map.offset),
            _ => {
                let path = cp_dir.join(i.to_string());
                if path.exists() {
                    (path, 0u64)
                } else {
                    info!(
                        "skipping maps[{i}] at {path:?} because it had no associated checkpoint file"
                    );
                    continue;
                }
            }
        };

        let path_ptr = data.len() as u64;
        let raw_path = CString::new(file_path.to_str().unwrap())?;
        data.extend(raw_path.as_bytes_with_nul());

        mmap_args.push((addr, len, prot, flags, path_ptr, offset));
    }

    {
        use iced_x86::code_asm::*;

        let mut c = CodeAssembler::new(64)?;

        // umap everything but our own code section
        // We make the assumption that our required space is vaddr to vaddr + 0x1000
        let code_len = 0x1000;
        c.xor(rdi, rdi)?;
        c.mov(rsi, vaddr)?;
        c.mov(rax, SYS_munmap)?;
        c.syscall()?;

        c.mov(rdi, vaddr + code_len)?;
        c.mov(rsi, u64::MAX - (vaddr + code_len))?;
        c.mov(rax, SYS_munmap)?;
        c.syscall()?;

        // Now go through and mmap in all the checkpoint mappings
        // TODO: this loop shouldn't be unrolled
        for (addr, len, prot, flags, path_ptr, offset) in mmap_args {
            let path_ptr = data_addr + path_ptr;

            // open the file
            c.mov(rdi, path_ptr)?;
            c.mov(rsi, O_RDONLY as u64)?;
            c.mov(rdx, 0o666u64)?;
            c.mov(rax, SYS_open)?;
            c.syscall()?;

            // mmap it in
            c.mov(rdi, addr)?;
            c.mov(rsi, len)?;
            c.mov(rdx, prot as u64)?;
            c.mov(r10, flags as u64)?;
            c.mov(r8, rax)?;
            c.mov(r9, offset)?;
            c.mov(rax, SYS_mmap)?;
            c.syscall()?;

            // close the file
            c.mov(rdi, 3u64)?;
            c.mov(rax, SYS_close)?;
            c.syscall()?;
        }

        // have the bootstrapper stop itself
        c.mov(rax, SYS_getpid)?;
        c.syscall()?;
        c.mov(rdi, rax)?;
        c.mov(rsi, SIGSTOP as u64)?;
        c.mov(rax, SYS_kill)?;
        c.syscall()?;

        // Loop infinitely
        let mut loop_loc = c.create_label();
        c.set_label(&mut loop_loc)?;
        c.jmp(loop_loc)?;

        let entry = data_addr + data.len() as u64;
        Ok((data, c.assemble(entry)?))
    }
}

pub fn restore_checkpoint(path: &PathBuf) -> Result<(), Box<dyn Error>> {
    info!("Restoring checkpoint from {path:?}");

    // Read in the last checkpoint
    let step = StepData::open(path)?;
    if step.seq == 0 {
        return Err("No checkpoints found".into());
    }

    let cp_path = path.join(step.seq.to_string());
    info!("Reading in last checkpoint data from {cp_path:?}");

    let regs: Registers = serde_json::from_reader(File::open(cp_path.join("regs"))?)?;
    let maps: Vec<MemoryMap> = serde_json::from_reader(File::open(cp_path.join("maps"))?)?;
    
    // Create the bootstrapper for the last checkpoint
    info!("Creating bootstrapper binary");
    let bs_path = cp_path.join("bs");
    create_bootstrapper(&bs_path, &cp_path, maps)?;

    // Run the bootstrapper
    info!("Running bootstrapper");
    let mut bootstrap = Command::new(&bs_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    {
        let mut ptrace = PTrace::new(bootstrap.id() as pid_t);
        ptrace.wait_pause_unattached()?;

        ptrace.attach()?;
        ptrace.set_regs(regs)?;
        ptrace.detach()?;

        info!("The process is fully restored");
        ptrace.resume()?;
    }

    // The bootstrapper should now be the restored process
    // let it run itself out
    let res = bootstrap.wait()?;

    info!("Restored process exited with status {res}, exiting");
    unsafe { exit(res.code().unwrap_or(0)) };
}
