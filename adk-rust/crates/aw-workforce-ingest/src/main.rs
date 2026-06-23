fn main() {
    let code = match aw_workforce_ingest::run_from_args() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}
