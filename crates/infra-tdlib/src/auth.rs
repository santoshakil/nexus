use nexus_error::AgentError;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::client::TdClient;

pub struct AuthConfig {
    pub api_id: i32,
    pub api_hash: String,
    pub db_dir: String,
    pub files_dir: String,
}

pub async fn wait_for_ready(
    client: &TdClient,
    auth_rx: &mut mpsc::UnboundedReceiver<Value>,
    config: &AuthConfig,
) -> Result<(), AgentError> {
    loop {
        let update = auth_rx
            .recv()
            .await
            .ok_or_else(|| AgentError::internal("auth channel closed"))?;

        let state = update
            .get("authorization_state")
            .and_then(|s| s.get("@type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        debug!(state, "auth state update");

        match state {
            "authorizationStateWaitTdlibParameters" => {
                info!("sending TDLib parameters");
                client
                    .send(json!({
                        "@type": "setTdlibParameters",
                        "database_directory": config.db_dir,
                        "files_directory": config.files_dir,
                        "api_id": config.api_id,
                        "api_hash": config.api_hash,
                        "system_language_code": "en",
                        "device_model": "Nexus Agent",
                        "application_version": "0.1.0",
                        "use_message_database": true,
                        "use_secret_chats": false,
                        "use_chat_info_database": true,
                        "use_file_database": true,
                    }))
                    .await?;
            }
            "authorizationStateReady" => {
                info!("authenticated successfully");
                return Ok(());
            }
            "authorizationStateClosed" => {
                return Err(AgentError::session("TDLib session closed"));
            }
            "authorizationStateClosing" => {
                debug!("session closing...");
            }
            other => {
                return Err(AgentError::auth(format!(
                    "unexpected auth state: {other} — run `nexus auth telegram` first"
                )));
            }
        }
    }
}

pub async fn interactive_auth(
    client: &TdClient,
    auth_rx: &mut mpsc::UnboundedReceiver<Value>,
    config: &AuthConfig,
) -> Result<(), AgentError> {
    loop {
        let update = auth_rx
            .recv()
            .await
            .ok_or_else(|| AgentError::internal("auth channel closed"))?;

        let state = update
            .get("authorization_state")
            .and_then(|s| s.get("@type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        debug!(state, "auth state update");

        match state {
            "authorizationStateWaitTdlibParameters" => {
                info!("sending TDLib parameters");
                client
                    .send(json!({
                        "@type": "setTdlibParameters",
                        "database_directory": config.db_dir,
                        "files_directory": config.files_dir,
                        "api_id": config.api_id,
                        "api_hash": config.api_hash,
                        "system_language_code": "en",
                        "device_model": "Nexus Agent",
                        "application_version": "0.1.0",
                        "use_message_database": true,
                        "use_secret_chats": false,
                        "use_chat_info_database": true,
                        "use_file_database": true,
                    }))
                    .await?;
            }
            "authorizationStateWaitPhoneNumber" => {
                let phone = prompt_stdin("Enter phone number (with country code): ")?;
                client
                    .send(json!({
                        "@type": "setAuthenticationPhoneNumber",
                        "phone_number": phone.trim(),
                    }))
                    .await?;
            }
            "authorizationStateWaitCode" => {
                let code = prompt_stdin("Enter the code you received: ")?;
                client
                    .send(json!({
                        "@type": "checkAuthenticationCode",
                        "code": code.trim(),
                    }))
                    .await?;
            }
            "authorizationStateWaitPassword" => {
                let hint = update
                    .get("authorization_state")
                    .and_then(|s| s.get("password_hint"))
                    .and_then(|h| h.as_str())
                    .unwrap_or("");
                if !hint.is_empty() {
                    eprintln!("Password hint: {hint}");
                }
                let password = prompt_stdin("Enter 2FA password: ")?;
                client
                    .send(json!({
                        "@type": "checkAuthenticationPassword",
                        "password": password.trim(),
                    }))
                    .await?;
            }
            "authorizationStateReady" => {
                info!("authenticated successfully");
                return Ok(());
            }
            "authorizationStateClosed" => {
                return Err(AgentError::session("session closed"));
            }
            other => {
                debug!(state = other, "unhandled auth state");
            }
        }
    }
}

fn prompt_stdin(prompt: &str) -> Result<String, AgentError> {
    eprint!("{prompt}");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| AgentError::internal(format!("stdin read failed: {e}")))?;
    if input.trim().is_empty() {
        return Err(AgentError::invalid_input("empty input"));
    }
    Ok(input)
}
