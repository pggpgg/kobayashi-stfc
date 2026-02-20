use std::process;

use kobayashi::cli;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    process::exit(cli::run_with_args(&args));
}
