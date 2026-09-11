use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("serve") => match justlocation_backend::transport::serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        Some(mode @ ("request" | "cells")) => {
            let result = std::env::args()
                .nth(2)
                .ok_or_else(|| "missing base64 request".to_owned())
                .and_then(|request| {
                    if mode == "cells" {
                        justlocation_backend::cell_service::request(&request)
                    } else {
                        justlocation_backend::transport::request(&request)
                    }
                });
            match result {
                Ok(response) => {
                    print!("{response}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("stdio") => {
            let result = justlocation_backend::protocol::Control::default()
                .serve(std::io::stdin().lock(), std::io::stdout().lock());
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("--version") => {
            println!("justlocationd {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help") | Some("-h") => {
            println!(
                "justlocationd serve | request <base64-json> | cells <base64-json> | stdio | --version"
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Use --help for available commands.");
            ExitCode::from(2)
        }
    }
}
