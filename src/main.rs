//! dfm CLI entry point: parse arguments, dispatch, and map the result to an
//! exit code.

fn main() {
    let code = dfm::cli::run(std::env::args());
    std::process::exit(code);
}
