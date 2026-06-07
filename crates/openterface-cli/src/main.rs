//! The `openterface-rs` command-line frontend.
//!
//! The full clap command surface (`connect` / `scan` / `status` / `reset`)
//! lands in W4.1 (its contract is drafted in W1.4). W0 ships a minimal entry
//! point so the workspace builds and `--version` works.

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("openterface-rs {}", env!("CARGO_PKG_VERSION"));
        }
        _ => {
            eprintln!(
                "openterface-rs {} — command surface lands in W4.1",
                env!("CARGO_PKG_VERSION")
            );
        }
    }
}
