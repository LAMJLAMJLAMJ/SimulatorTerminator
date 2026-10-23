//! Test helper for the Python harness: reads one sample per line from stdin
//! and prints the parameters as JSON.
//!
//!   cargo run --release -p ra-metrics --example metrics_json -- --fs 48000 [--energy] [--no-onset] [--sti]
//!
//! With --sti, stdin holds seven whitespace-separated columns of squared
//! pressure, one per octave band.

use ra_metrics::{Options, band_from_energy, band_from_ir, mtf_band, sti_from_mtf};
use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let fs: f64 = args
        .iter()
        .position(|a| a == "--fs")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .expect("--fs <rate> is required");
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("read stdin");
    let rows: Vec<Vec<f64>> = input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(|v| v.parse().expect("number")).collect())
        .collect();

    if flag("--sti") {
        let mtf: [[f64; 14]; 7] =
            std::array::from_fn(|k| mtf_band(&rows.iter().map(|r| r[k]).collect::<Vec<_>>(), fs));
        println!("{{\"STI\": {}}}", sti_from_mtf(&mtf));
        return;
    }
    let x: Vec<f64> = rows.iter().map(|r| r[0]).collect();
    let opts = Options { skip_onset: flag("--no-onset") };
    let p = if flag("--energy") { band_from_energy(&x, fs, &opts) } else { band_from_ir(&x, fs, &opts) };
    let t = |v: Option<f64>| v.map_or("null".to_string(), |v| v.to_string());
    println!(
        "{{\"EDT\": {}, \"T20\": {}, \"T30\": {}, \"C50\": {}, \"C80\": {}, \"D50\": {}, \"Ts\": {}}}",
        t(p.edt),
        t(p.t20),
        t(p.t30),
        p.c50,
        p.c80,
        p.d50,
        p.ts
    );
}
