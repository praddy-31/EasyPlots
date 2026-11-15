//! The standalone `easyplots` program; all the work is in `lib.rs`.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::ExitCode::from(easyplots_cli::run(&args) as u8)
}
