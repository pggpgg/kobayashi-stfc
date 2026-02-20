use std::env;

use kobayashi::server;

#[derive(Debug, Clone, Copy)]
enum Command {
    Serve,
    Simulate,
    Optimize,
    Import,
    Validate,
}

fn parse_command() -> Option<Command> {
    match env::args().nth(1).as_deref() {
        Some("serve") => Some(Command::Serve),
        Some("simulate") => Some(Command::Simulate),
        Some("optimize") => Some(Command::Optimize),
        Some("import") => Some(Command::Import),
        Some("validate") => Some(Command::Validate),
        _ => None,
    }
}

fn main() {
    match parse_command() {
        Some(Command::Serve) => {
            let bind_addr =
                env::var("KOBAYASHI_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
            if let Err(err) = server::run_server(&bind_addr) {
                eprintln!("server error: {err}");
            }
        }
        Some(Command::Simulate) => println!("simulate stub"),
        Some(Command::Optimize) => println!("optimize stub"),
        Some(Command::Import) => println!("import stub"),
        Some(Command::Validate) => println!("validate stub"),
        None => {
            eprintln!("usage: kobayashi <serve|simulate|optimize|import|validate>");
        }
    }
}
