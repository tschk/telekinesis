fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("tk {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Err(e) = telekinesis::run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
