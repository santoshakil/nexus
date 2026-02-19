mod mcp;

use std::env;
use std::sync::Arc;

use nexus_discord::{DiscordAdapter, DiscordConfig};
use nexus_google::{GmailAdapter, GmailConfig};
use nexus_messaging::AgentService;
use nexus_slack::{SlackAdapter, SlackConfig};
use nexus_tdlib::{AuthConfig, TdClient, TdlibAdapter};
use nexus_whatsapp::{WhatsAppAdapter, WhatsAppConfig};
use tracing::{error, info};

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nexus=info")),
        )
        .compact()
        .init();
}

fn load_telegram_config() -> Result<AuthConfig, String> {
    let api_id: i32 = env::var("TELEGRAM_API_ID")
        .map_err(|_| "TELEGRAM_API_ID env var not set".to_string())?
        .parse()
        .map_err(|e| format!("invalid TELEGRAM_API_ID: {e}"))?;

    let api_hash = env::var("TELEGRAM_API_HASH")
        .map_err(|_| "TELEGRAM_API_HASH env var not set".to_string())?;

    let data_dir = env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{home}/.nexus")
    });

    let db_dir = format!("{data_dir}/tdlib");
    let files_dir = format!("{data_dir}/tdlib/files");

    std::fs::create_dir_all(&db_dir)
        .map_err(|e| format!("failed to create {db_dir}: {e}"))?;
    std::fs::create_dir_all(&files_dir)
        .map_err(|e| format!("failed to create {files_dir}: {e}"))?;

    Ok(AuthConfig {
        api_id,
        api_hash,
        db_dir,
        files_dir,
    })
}

fn load_gmail_config() -> Result<GmailConfig, String> {
    let email = env::var("GMAIL_ADDRESS")
        .map_err(|_| "GMAIL_ADDRESS env var not set".to_string())?;
    let app_password = env::var("GMAIL_APP_PASSWORD")
        .map_err(|_| "GMAIL_APP_PASSWORD env var not set".to_string())?;
    Ok(GmailConfig {
        email,
        app_password,
    })
}

fn load_whatsapp_config() -> Result<WhatsAppConfig, String> {
    let access_token = env::var("WHATSAPP_ACCESS_TOKEN")
        .map_err(|_| "WHATSAPP_ACCESS_TOKEN env var not set".to_string())?;
    let phone_number_id = env::var("WHATSAPP_PHONE_NUMBER_ID")
        .map_err(|_| "WHATSAPP_PHONE_NUMBER_ID env var not set".to_string())?;
    Ok(WhatsAppConfig {
        access_token,
        phone_number_id,
    })
}

fn load_slack_config() -> Result<SlackConfig, String> {
    let bot_token = env::var("SLACK_BOT_TOKEN")
        .map_err(|_| "SLACK_BOT_TOKEN env var not set".to_string())?;
    Ok(SlackConfig { bot_token })
}

fn load_discord_config() -> Result<DiscordConfig, String> {
    let bot_token = env::var("DISCORD_BOT_TOKEN")
        .map_err(|_| "DISCORD_BOT_TOKEN env var not set".to_string())?;
    Ok(DiscordConfig { bot_token })
}

async fn run_auth_telegram() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_telegram_config().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    eprintln!("Nexus — Telegram Authentication");
    eprintln!("================================");
    eprintln!("TDLib database: {}", config.db_dir);
    eprintln!();

    let client = TdClient::new();
    let mut auth_rx = client
        .take_auth_rx()
        .ok_or("failed to get auth receiver")?;

    nexus_tdlib::auth::interactive_auth(&client, &mut auth_rx, &config).await?;

    let me = client
        .send(serde_json::json!({"@type": "getMe"}))
        .await?;

    let name = me
        .get("first_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let username = me
        .get("usernames")
        .and_then(|u| u.get("active_usernames"))
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|u| u.as_str())
        .unwrap_or("none");

    eprintln!();
    eprintln!("Authenticated as: {name} (@{username})");
    eprintln!("Session saved. You can now run `nexus mcp`");

    Ok(())
}

async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let mut agent = AgentService::new();

    let telegram = match load_telegram_config() {
        Ok(cfg) => {
            let client = Arc::new(TdClient::new());
            let mut auth_rx = client
                .take_auth_rx()
                .ok_or("failed to get auth receiver")?;

            nexus_tdlib::auth::wait_for_ready(&client, &mut auth_rx, &cfg).await?;
            info!("telegram connected");

            let adapter = Arc::new(TdlibAdapter::new(client));
            agent.register(adapter.clone());
            Some(adapter)
        }
        Err(e) => {
            info!("telegram not configured: {e}");
            None
        }
    };

    let gmail = match load_gmail_config() {
        Ok(cfg) => {
            let adapter = Arc::new(GmailAdapter::new(cfg));
            agent.register(adapter.clone());
            info!("gmail configured");
            Some(adapter)
        }
        Err(e) => {
            info!("gmail not configured: {e}");
            None
        }
    };

    let whatsapp = match load_whatsapp_config() {
        Ok(cfg) => {
            let adapter = Arc::new(WhatsAppAdapter::new(cfg));
            agent.register(adapter.clone());
            info!("whatsapp configured");
            Some(adapter)
        }
        Err(e) => {
            info!("whatsapp not configured: {e}");
            None
        }
    };

    let slack = match load_slack_config() {
        Ok(cfg) => {
            let adapter = Arc::new(SlackAdapter::new(cfg));
            agent.register(adapter.clone());
            info!("slack configured");
            Some(adapter)
        }
        Err(e) => {
            info!("slack not configured: {e}");
            None
        }
    };

    let discord = match load_discord_config() {
        Ok(cfg) => {
            let adapter = Arc::new(DiscordAdapter::new(cfg));
            agent.register(adapter.clone());
            info!("discord configured");
            Some(adapter)
        }
        Err(e) => {
            info!("discord not configured: {e}");
            None
        }
    };

    let server = mcp::McpServer::new(agent, telegram, gmail, whatsapp, slack, discord);
    server.run().await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    init_tracing();

    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("mcp");

    let result = match cmd {
        "auth" => {
            let platform = args.get(2).map(|s| s.as_str()).unwrap_or("telegram");
            match platform {
                "telegram" | "tg" => run_auth_telegram().await,
                other => {
                    eprintln!("Unknown platform for auth: {other}");
                    eprintln!("Usage: nexus auth [telegram]");
                    return;
                }
            }
        }
        "mcp" => run_mcp_server().await,
        "help" | "--help" | "-h" => {
            eprintln!("Nexus — Universal Agent Tools Platform");
            eprintln!();
            eprintln!("Usage:");
            eprintln!("  nexus auth telegram   Authenticate with Telegram (interactive)");
            eprintln!("  nexus mcp             Start MCP server (stdio, for Claude Code)");
            eprintln!("  nexus help            Show this help");
            eprintln!();
            eprintln!("Environment variables:");
            eprintln!("  TELEGRAM_API_ID          Telegram API ID (from my.telegram.org)");
            eprintln!("  TELEGRAM_API_HASH        Telegram API hash");
            eprintln!("  GMAIL_ADDRESS            Gmail address");
            eprintln!("  GMAIL_APP_PASSWORD       Gmail app password");
            eprintln!("  WHATSAPP_ACCESS_TOKEN    WhatsApp Business Cloud API token");
            eprintln!("  WHATSAPP_PHONE_NUMBER_ID WhatsApp sender phone number ID");
            eprintln!("  SLACK_BOT_TOKEN          Slack Bot User OAuth Token (xoxb-...)");
            eprintln!("  DISCORD_BOT_TOKEN        Discord Bot token");
            eprintln!("  NEXUS_DATA_DIR           Data directory (default: ~/.nexus)");
            eprintln!("  RUST_LOG                 Log level (default: nexus=info)");
            Ok(())
        }
        unknown => {
            eprintln!("Unknown command: {unknown}");
            eprintln!("Run `nexus help` for usage");
            return;
        }
    };

    if let Err(e) = result {
        error!(%e, "fatal error");
        std::process::exit(1);
    }
}
