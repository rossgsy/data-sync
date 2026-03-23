use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG: &str = "config.yaml";

#[derive(Serialize, Deserialize)]
struct SyncpondConfig {
    command_api_key: String,
    ws_addr: String,
    command_addr: String,
    jwt_key: Option<String>,
}

fn default_config() -> SyncpondConfig {
    SyncpondConfig {
        command_api_key: "change-me".to_string(),
        ws_addr: "127.0.0.1:8080".to_string(),
        command_addr: "127.0.0.1:9090".to_string(),
        jwt_key: None,
    }
}

#[derive(Parser, Debug)]
#[command(name = "syncpond-server-cli")]
#[command(about = "Interact with syncpond-server command protocol", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a default config file in current directory (or --config path)
    Init {
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Send a single command to syncpond-server command socket
    Send {
        #[arg(short = 'm', long)]
        command: String,

        #[arg(short = 'c', long)]
        config: Option<PathBuf>,

        #[arg(short = 'a', long)]
        command_addr: Option<String>,

        #[arg(short = 'k', long)]
        api_key: Option<String>,
    },
    /// Generate a JWT for room and containers using TOKEN.GEN
    Token {
        #[arg(short = 'r', long)]
        room_id: u64,

        #[arg(short = 'C', long)]
        containers: Vec<String>,

        #[arg(short, long)]
        config: Option<PathBuf>,

        #[arg(short, long)]
        command_addr: Option<String>,

        #[arg(short, long)]
        api_key: Option<String>,
    },
    /// Interactive command prompt for syncpond-server protocol
    Interactive {
        #[arg(short, long)]
        config: Option<PathBuf>,

        #[arg(short, long)]
        command_addr: Option<String>,

        #[arg(short, long)]
        api_key: Option<String>,
    },
}

fn load_config(config_path: &Path) -> Result<SyncpondConfig> {
    let text = fs::read_to_string(config_path)
        .context(format!("failed to read config file {}", config_path.display()))?;
    let config: SyncpondConfig = serde_yaml::from_str(&text)
        .context(format!("failed to parse config yaml {}", config_path.display()))?;
    Ok(config)
}

fn send_command(command_addr: &str, api_key: &str, cmd: &str) -> Result<String> {
    let mut stream = TcpStream::connect(command_addr)
        .context(format!("failed to connect to {}", command_addr))?;

    stream
        .write_all(api_key.as_bytes())
        .context("failed to send api key")?;
    stream
        .write_all(b"\n")
        .context("failed to send newline after api key")?;

    stream
        .write_all(cmd.as_bytes())
        .context("failed to send command")?;
    stream
        .write_all(b"\n")
        .context("failed to send newline after command")?;

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .context("failed to read response")?;

    Ok(response.trim_end().to_string())
}

fn interactive_mode(command_addr: &str, api_key: &str) -> Result<()> {
    println!("Entering interactive mode. Type 'exit' or 'quit' to leave.");
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
            println!("closing interactive shell");
            break;
        }

        if trimmed.is_empty() {
            continue;
        }

        let result = send_command(command_addr, api_key, trimmed)?;
        println!("=> {}", result);
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { config } => {
            let config_path = config.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));
            if config_path.exists() {
                return Err(anyhow!("config file already exists: {}", config_path.display()));
            }
            let template = default_config();
            let yaml = serde_yaml::to_string(&template)?;
            fs::write(&config_path, yaml)
                .context(format!("failed to write config to {}", config_path.display()))?;
            println!("created config at {}", config_path.display());
            Ok(())
        }
        Commands::Send {
            command,
            config,
            command_addr,
            api_key,
        } => {
            let config_path = config.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));
            let config = load_config(&config_path)?;

            let addr = command_addr.as_deref().unwrap_or(&config.command_addr);
            let key = api_key.as_deref().unwrap_or(&config.command_api_key);

            let response = send_command(addr, key, &command)?;
            println!("{}", response);
            Ok(())
        }
        Commands::Token {
            room_id,
            containers,
            config,
            command_addr,
            api_key,
        } => {
            let config_path = config.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));
            let config = load_config(&config_path)?;

            let addr = command_addr.as_deref().unwrap_or(&config.command_addr);
            let key = api_key.as_deref().unwrap_or(&config.command_api_key);

            let cmd = if containers.is_empty() {
                format!("TOKEN.GEN {}", room_id)
            } else {
                format!("TOKEN.GEN {} {}", room_id, containers.join(" "))
            };

            let response = send_command(addr, key, &cmd)?;
            println!("{}", response);
            Ok(())
        }
        Commands::Interactive {
            config,
            command_addr,
            api_key,
        } => {
            let config_path = config.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));
            let config = load_config(&config_path)?;

            let addr = command_addr.as_deref().unwrap_or(&config.command_addr);
            let key = api_key.as_deref().unwrap_or(&config.command_api_key);

            interactive_mode(addr, key)
        }
    }
}
