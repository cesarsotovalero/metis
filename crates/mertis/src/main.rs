#![forbid(unsafe_code)]
use clap::{Parser, Subcommand, ValueEnum};
use cs_core::EventEnvelope;
use cs_store::Store;
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};
#[derive(Parser)]
#[command(version, about = "Offline synthetic coding-agent session observer")]
struct Cli {
    #[arg(long, global = true, env = "MERTIS_HOME")]
    home: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Clone, ValueEnum)]
enum Agent {
    Fixture,
}
#[derive(Subcommand)]
enum Command {
    Import {
        #[arg(long, value_enum)]
        agent: Agent,
        #[arg(long)]
        path: PathBuf,
    },
    Inspect {
        id: String,
        #[arg(long)]
        json: bool,
    },
}
fn main() {
    if let Err(e) = run() {
        eprintln!("mertis: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let home = cli
        .home
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(|p| PathBuf::from(p).join("mertis")))
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share/mertis")))
        .ok_or("set --home or MERTIS_HOME")?;
    std::fs::create_dir_all(&home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut store = Store::open(&home.join("ledger.sqlite3"))?;
    match cli.command {
        Command::Import {
            agent: Agent::Fixture,
            path,
        } => {
            // Fixed 32 MiB file cap and 64 KiB line cap bound memory and parser work.
            let file = std::fs::File::open(path)?;
            if file.metadata()?.len() > 32 * 1024 * 1024 {
                return Err("fixture exceeds 32 MiB limit".into());
            }
            let reader = BufReader::new(file.take(32 * 1024 * 1024 + 1));
            let mut events = Vec::new();
            for (index, line) in reader.lines().enumerate() {
                let line = line?;
                if line.len() > 65536 {
                    return Err(format!("line {} exceeds limit", index + 1).into());
                }
                let e: EventEnvelope = serde_json::from_str(&line)
                    .map_err(|_| format!("invalid fixture at line {}", index + 1))?;
                e.validate()?;
                events.push(e);
            }
            // Validate accounting before committing the file to its persistent ledger.
            let mut validation = Store::open(std::path::Path::new(":memory:"))?;
            validation.import(&events)?;
            let ids = events
                .iter()
                .map(|e| e.session_id.clone())
                .collect::<std::collections::BTreeSet<_>>();
            for id in &ids {
                validation.inspect(id)?;
            }
            let inserted = store.import(&events)?;
            println!(
                "Synthetic fixture: {inserted} new events; sessions: {}",
                ids.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
        Command::Inspect { id, json } => {
            let r = store.inspect(&id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                print!("{}", r.markdown());
            }
        }
    }
    Ok(())
}
