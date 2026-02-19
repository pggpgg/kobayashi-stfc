mod combat;
mod data;
mod optimizer;
mod parallel;
mod server;

use std::env;

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
        Some(Command::Serve) => println!("serve stub"),
        Some(Command::Simulate) => println!("simulate stub"),
        Some(Command::Optimize) => println!("optimize stub"),
        Some(Command::Import) => println!("import stub"),
        Some(Command::Validate) => println!("validate stub"),
        None => {
            eprintln!("usage: kobayashi <serve|simulate|optimize|import|validate>");
        }
    }
}
