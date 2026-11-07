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
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    Inspect {
        id: String,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
enum DaemonCommand {
    /// Run the local observer daemon in the foreground.
    Start,
    /// Print authenticated daemon health as JSON.
    Status,
    /// Request graceful daemon shutdown.
    Stop,
}
fn daemon(
    home: &std::path::Path,
    command: DaemonCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(command, DaemonCommand::Start) {
        cs_daemon::run(home, cs_daemon::Config::default())?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        use cs_daemon::{
            protocol::{Operation, Reply},
            Client,
        };
        let mut client = Client::connect(
            home,
            "mertis-cli",
            &format!("cli-{}", std::process::id()),
            None,
        )?;
        let reply = match command {
            DaemonCommand::Status => client.request(Operation::Health {})?,
            DaemonCommand::Stop => client.request(Operation::Shutdown {})?,
            DaemonCommand::Start => unreachable!(),
        };
        if !matches!(reply, Reply::Health { .. } | Reply::Shutdown {}) {
            return Err("unexpected daemon control response".into());
        }
        println!("{}", serde_json::to_string_pretty(&reply)?);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("daemon IPC is currently supported on Linux only".into())
    }
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
    if let Command::Daemon { command } = cli.command {
        return daemon(&home, command);
    }
    std::fs::create_dir_all(&home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut store = Store::open(&home.join("ledger.sqlite3"))?;
    match cli.command {
        Command::Daemon { .. } => unreachable!(),
        Command::Import {
            agent: Agent::Fixture,
            path,
        } => {
            // Fixed 32 MiB file cap and 64 KiB line cap bound memory and parser work.
            let file = std::fs::File::open(path)?;
            if file.metadata()?.len() > 32 * 1024 * 1024 {
                return Err("fixture exceeds 32 MiB limit".into());
            }
            let mut reader = BufReader::new(file);
            let mut events = Vec::new();
            let mut bytes_read = 0_u64;
            loop {
                // Bound allocation before reading; allow CRLF outside the JSON record cap.
                let mut line = Vec::new();
                let read = (&mut reader).take(65539).read_until(b'\n', &mut line)?;
                if read == 0 {
                    break;
                }
                bytes_read += read as u64;
                if bytes_read > 32 * 1024 * 1024 {
                    return Err("fixture exceeds 32 MiB limit".into());
                }
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.len() > 65536 {
                    return Err(format!("line {} exceeds limit", events.len() + 1).into());
                }
                let e: EventEnvelope = serde_json::from_slice(&line)
                    .map_err(|_| format!("invalid fixture at line {}", events.len() + 1))?;
                e.validate()?;
                events.push(e);
            }
            let ids = events
                .iter()
                .map(|e| e.session_id.clone())
                .collect::<std::collections::BTreeSet<_>>();
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
