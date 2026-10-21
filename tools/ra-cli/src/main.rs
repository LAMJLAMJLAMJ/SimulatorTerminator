//! `ra-cli info <file.raproj>` prints a project's manifest and scene report.
//! `run` and `check` arrive with the ray engine and the validation cases.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, path] if cmd == "info" => match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| ra_project::read(&b).map_err(|e| e.to_string()))
        {
            Ok(p) => {
                println!("{}", serde_json::to_string_pretty(&p.manifest).unwrap());
                if let Some(s) = p.scene {
                    println!("{}", serde_json::to_string_pretty(&s.doc.report).unwrap());
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{path}: {e}");
                ExitCode::FAILURE
            }
        },
        [cmd] if cmd == "capabilities" => {
            println!("{}", serde_json::to_string_pretty(&ra_api::Engine::new().capabilities()).unwrap());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: ra-cli info <file.raproj> | ra-cli capabilities");
            ExitCode::from(2)
        }
    }
}
