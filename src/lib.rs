//! The `kairo` command line. A thin adapter: it parses arguments, calls
//! `kairo-core`, and prints. Both binaries, `kairo` and `kairodb`, call [`run`].

mod cli;
mod ui;

/// Parses the process arguments, runs the command, and exits non-zero on failure.
pub fn run() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the async runtime starts");

    if let Err(err) = runtime.block_on(cli::run()) {
        cli::print_failure(&err);
        std::process::exit(1);
    }
}
