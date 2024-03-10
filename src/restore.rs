use std::{error::Error, fs::File, path::PathBuf};

use crate::{checkpoint::StepData, ptrace::Registers};

pub fn restore_checkpoint(path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let step = StepData::open(path)?;
    if step.seq == 0 {
        return Err("No checkpoints found".into());
    }

    let cp_path = path.join(step.seq.to_string());
    let regs: Registers = serde_json::from_reader(File::open(cp_path.join("regs"))?)?;

    

    Ok(())
}
