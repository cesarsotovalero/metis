//! Send the checked-in cached session to an already running local daemon.
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use cs_daemon::{
        protocol::{Operation, Reply},
        Client,
    };
    let home = std::env::args_os()
        .nth(1)
        .ok_or("usage: cached_session <mertis-home>")?;
    let events = include_str!("../../../fixtures/cached.jsonl")
        .lines()
        .map(serde_json::from_str::<cs_core::EventEnvelope>)
        .collect::<Result<Vec<_>, _>>()?;
    let mut client = Client::connect(
        std::path::Path::new(&home),
        "cached",
        "epoch-1",
        Some("cached"),
    )?;
    if !matches!(
        client.request(Operation::AppendBatch { events })?,
        Reply::Appended { .. }
    ) {
        return Err("unexpected append response".into());
    }
    match client.request(Operation::Snapshot {})? {
        Reply::Snapshot { report } => println!("{}", serde_json::to_string_pretty(&report)?),
        _ => return Err("unexpected snapshot response".into()),
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("daemon example requires Linux");
    std::process::exit(1);
}
