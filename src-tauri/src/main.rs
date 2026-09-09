fn main() {
    if std::env::args().any(|argument| argument == "--native-messaging") {
        if let Err(error) = threadbox_lib::run_native_messaging() {
            eprintln!("Threadbox native messaging failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    threadbox_lib::run();
}
