//! `ra-cli info <file.raproj>` prints a project's manifest and scene report.
//! `ra-cli run <file.raproj> [request.json]` ray traces the project's sources and receivers
//! and prints the results; the request is a JSON `RunRequest`, merged over the defaults.
//! `check` arrives with the validation cases.

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
        [cmd, path, rest @ ..] if cmd == "run" && rest.len() <= 1 => match run(path, rest.first()) {
            Ok(results) => {
                println!("{results}");
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
            eprintln!(
                "usage: ra-cli info <file.raproj> | ra-cli run <file.raproj> [request.json] | ra-cli capabilities"
            );
            ExitCode::from(2)
        }
    }
}

fn run(path: &str, request: Option<&String>) -> Result<String, String> {
    let mut engine = ra_api::Engine::new();
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let (opened, _) = engine.handle_json(r#"{"type":"open_project"}"#, vec![bytes], &mut |_| {});
    let opened: serde_json::Value = serde_json::from_str(&opened).map_err(|e| e.to_string())?;
    if opened["type"] == "error" {
        return Err(opened["message"].to_string());
    }
    let mut run = serde_json::json!({
        "solver": "rt",
        "bands": opened["manifest"]["bands"],
        "max_order": 0,
        "n_rays": 20000,
        "seed": 1,
        "duration_s": 3.0,
        "receivers": [],
    });
    if let Some(file) = request {
        let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        let extra: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
        for (k, v) in extra.as_object().ok_or("the request must be a JSON object")? {
            run[k] = v.clone();
        }
    }
    let cmd = serde_json::json!({ "type": "run", "run": run }).to_string();
    let (reply, _) = engine.handle_json(&cmd, vec![], &mut |_| {});
    let reply: serde_json::Value = serde_json::from_str(&reply).map_err(|e| e.to_string())?;
    if reply["type"] == "error" {
        return Err(reply["message"].to_string());
    }
    serde_json::to_string_pretty(&reply["results"]).map_err(|e| e.to_string())
}
