use std::{
    error::Error,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use goblin::{
    container::{Container, Ctx, Endian},
    elf::{
        header::{header64, EM_X86_64, ET_EXEC},
        program_header::{program_header64, PF_R, PF_W, PF_X},
        Header, ProgramHeader,
    },
};
use iced_x86::code_asm::CodeAssembler;
use procfs::process::MemoryMap;
use scroll::Pwrite;

use crate::{checkpoint::StepData, ptrace::Registers};

// TODO: more portability, this whole thing is pretty messy
pub fn write_bootstrapper(output_path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    // https://github.com/tchajed/minimal-elf/

    let vaddr = 0xe0000;
    let header_size = header64::SIZEOF_EHDR as u64;
    let pheader_size = program_header64::SIZEOF_PHDR as u64;
    let program_offset: u64 = header_size + pheader_size;
    let entry = vaddr + program_offset;

    let program = make_program(todo!(), vaddr, entry)?;

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

    let mut buf = vec![0u8; (header_size + pheader_size) as usize];
    buf.pwrite(header, 0).unwrap();
    buf.pwrite(pheader, header_size as usize).unwrap();

    let mut outfile = File::create(output_path)?;
    outfile.write_all(&buf)?;
    outfile.write_all(&program)?;

    Ok(())
}

pub fn make_program(maps: Vec<MemoryMap>, vaddr: u64, entry: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    // let mut mmap_args = vec![];
    // for (i, map) in maps.into_iter().enumerate() {
        
    // }

    // let mut c = CodeAssembler::new(64)?;
    
    // Ok(c.assemble(entry)?)
    todo!()
}

pub fn restore_checkpoint(path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let step = StepData::open(path)?;
    if step.seq == 0 {
        return Err("No checkpoints found".into());
    }

    let cp_path = path.join(step.seq.to_string());
    let regs: Registers = serde_json::from_reader(File::open(cp_path.join("regs"))?)?;

    Ok(())
}
