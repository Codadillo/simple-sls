# Pure SLS
A simple, userspace SLS implementation that runs on (almost) pure programs.

## Dependencies
Rust and an amd64 Linux system.

## Running
To start periodically checkpointing a process, simply run `cargo run --release checkpoint --pid <process_pid> --period 1`. 
See `cargo run --release checkpoint --help` for a full list of options.

To restore a process from its most recent checkpoint, run `cargo run --release restore`. 
See `cargo run --release restore --help` for more details.

The `examples/` directory contains several example programs that can be effectively checkpointed and restored, which can be run with `cargo run --release --example <example_name>`. 

## Evaluation
The evaluation used for the SLS is in the `evaluations/` directory. It is its own crate, and each evaluation is a different binary. 
