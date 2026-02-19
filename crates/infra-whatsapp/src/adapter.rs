use std::path::Path;

use async_trait::async_trait;
use nexus_domain::*;
use nexus_error::AgentError;
use reqwest::multipart;
use serde::Deserialize;
use tracing::debug;

const BASE_URL: &str = "https://graph.facebook.com/v21.0";

pub struct WhatsAppConfig {
    pub access_token: String,
    pub phone_number_id: String,
}

pub struct WhatsAppAdapter {
    base_url: String,
    auth: String,
    phone_number_id: String,
    http: reqwest::Client,
}

impl WhatsAppAdapter {
    pub fn new(cfg: WhatsAppConfig) -> Self {
        let base_url = format!("{BASE_URL}/{}", cfg.phone_number_id);
        let auth = format!("Bearer {}", cfg.access_token);
        let phone_number_id = cfg.phone_number_id;
        Self {
            base_url,
            auth,
            phone_number_id,
            http: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn api_get(&self, url: &str) -> Result<serde_json::Value, AgentError> {
        debug!(url, "whatsapp GET");
        let resp = self
            .http
            .get(url)
            .header("Authorization", &self.auth)
            .send()
            .await
            .map_err(|e| AgentError::network(format!("whatsapp request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| AgentError::network(format!("whatsapp read body: {e}")))?;

        if !status.is_success() {
            return Err(parse_api_error(&body, status.as_u16()));
        }

        serde_json::from_str(&body)
            .map_err(|e| AgentError::api(format!("whatsapp parse response: {e}")))
    }

    async fn api_post_json(
        &self,
        url: &str,
        json: &serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        debug!(url, "whatsapp POST json");
        let resp = self
            .http
            .post(url)
            .header("Authorization", &self.auth)
            .json(json)
            .send()
            .await
            .map_err(|e| AgentError::network(format!("whatsapp request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| AgentError::network(format!("whatsapp read body: {e}")))?;

        if !status.is_success() {
            return Err(parse_api_error(&body, status.as_u16()));
        }

        serde_json::from_str(&body)
            .map_err(|e| AgentError::api(format!("whatsapp parse response: {e}")))
    }

    async fn upload_media(
        &self,
        file_path: &str,
        mime: &str,
    ) -> Result<String, AgentError> {
        let url = self.url("/media");
        debug!(url, file_path, mime, "whatsapp upload media");

        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| AgentError::api(format!("read file {file_path}: {e}")))?;

        let file_name = Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let file_part = multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str(mime)
            .map_err(|e| AgentError::api(format!("invalid mime type: {e}")))?;

        let form = multipart::Form::new()
            .text("messaging_product", "whatsapp")
            .part("file", file_part);

        let resp = self
            .http
            .post(&url)
            .header("Authorization", &self.auth)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AgentError::network(format!("whatsapp upload failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| AgentError::network(format!("whatsapp read body: {e}")))?;

        if !status.is_success() {
            return Err(parse_api_error(&body, status.as_u16()));
        }

        let parsed: MediaUploadResponse = serde_json::from_str(&body)
            .map_err(|e| AgentError::api(format!("whatsapp parse upload response: {e}")))?;

        Ok(parsed.id)
    }
}

fn parse_api_error(body: &str, status: u16) -> AgentError {
    if status == 401 || status == 403 {
        return AgentError::auth(format!("whatsapp auth failed ({status}): {body}"));
    }

    #[derive(Deserialize)]
    struct WaErrorResp {
        error: Option<WaErrorDetail>,
    }
    #[derive(Deserialize)]
    struct WaErrorDetail {
        message: Option<String>,
        code: Option<i64>,
    }

    if let Ok(err) = serde_json::from_str::<WaErrorResp>(body) {
        if let Some(detail) = err.error {
            let msg = detail.message.unwrap_or_else(|| body.to_string());
            let code = detail.code.unwrap_or(0);
            return AgentError::api(format!("whatsapp error {code}: {msg}"));
        }
    }

    AgentError::api(format!("whatsapp error ({status}): {body}"))
}

fn detect_media_type(file_path: &str) -> (&'static str, &'static str) {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => ("image", "image/jpeg"),
        "png" => ("image", "image/png"),
        "gif" => ("image", "image/gif"),
        "webp" => ("image", "image/webp"),
        "mp4" => ("video", "video/mp4"),
        "3gp" => ("video", "video/3gpp"),
        "mp3" => ("audio", "audio/mpeg"),
        "ogg" => ("audio", "audio/ogg"),
        "amr" => ("audio", "audio/amr"),
        "aac" => ("audio", "audio/aac"),
        "opus" => ("audio", "audio/opus"),
        "pdf" => ("document", "application/pdf"),
        "doc" => ("document", "application/msword"),
        "docx" => ("document", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xls" => ("document", "application/vnd.ms-excel"),
        "xlsx" => ("document", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "ppt" => ("document", "application/vnd.ms-powerpoint"),
        "pptx" => ("document", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        "txt" => ("document", "text/plain"),
        "zip" => ("document", "application/zip"),
        _ => ("document", "application/octet-stream"),
    }
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct MediaUploadResponse {
    id: String,
}

#[derive(Deserialize)]
struct SendMessageResponse {
    messages: Vec<SendMessageEntry>,
}

#[derive(Deserialize)]
struct SendMessageEntry {
    id: String,
}

#[async_trait]
impl MessagingPort for WhatsAppAdapter {
    fn platform(&self) -> Platform {
        Platform::WhatsApp
    }

    async fn get_profile(&self) -> Result<Profile, AgentError> {
        let url = self.url(
            "/whatsapp_business_profile?fields=about,address,description,email,profile_picture_url,messaging_product,vertical",
        );
        let resp = self.api_get(&url).await?;

        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first());

        let name = data
            .and_then(|d| d.get("about"))
            .and_then(|v| v.as_str())
            .unwrap_or("WhatsApp Business")
            .to_string();

        let email = data
            .and_then(|d| d.get("email"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);

        Ok(Profile {
            platform: Platform::WhatsApp,
            id: self.phone_number_id.clone(),
            name,
            username: None,
            email,
            phone: None,
        })
    }

    async fn list_channels(&self, _limit: usize) -> Result<Vec<Channel>, AgentError> {
        Err(AgentError::not_implemented(
            "WhatsApp Cloud API does not support listing conversations. Use send_message with a phone number directly.",
        ))
    }

    async fn read_messages(
        &self,
        _channel: &str,
        _limit: usize,
        _cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        Err(AgentError::not_implemented(
            "WhatsApp Cloud API does not support reading message history. Messages are delivered via webhooks.",
        ))
    }

    async fn send_message(
        &self,
        channel: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<Message, AgentError> {
        let url = self.url("/messages");

        let mut body = serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": channel,
            "type": "text",
            "text": { "body": text }
        });

        if let Some(rt) = reply_to {
            body["context"] = serde_json::json!({ "message_id": rt });
        }

        let resp = self.api_post_json(&url, &body).await?;

        let parsed: SendMessageResponse = serde_json::from_value(resp)
            .map_err(|e| AgentError::api(format!("whatsapp parse send response: {e}")))?;

        let msg_id = parsed
            .messages
            .into_iter()
            .next()
            .map(|m| m.id)
            .ok_or_else(|| AgentError::api("whatsapp: no message id in response"))?;

        Ok(Message {
            id: msg_id,
            platform: Platform::WhatsApp,
            channel_id: channel.to_string(),
            sender: self.phone_number_id.clone(),
            text: text.to_string(),
            timestamp: now_ts(),
            has_attachment: false,
            reply_to: reply_to.map(|s| s.to_string()),
            meta: MessageMeta::default(),
        })
    }

    async fn search(
        &self,
        _query: &str,
        _limit: usize,
        _cursor: Option<&str>,
    ) -> Result<Paginated<Message>, AgentError> {
        Err(AgentError::not_implemented(
            "WhatsApp Cloud API does not support message search.",
        ))
    }
}

#[async_trait]
impl WhatsAppExt for WhatsAppAdapter {
    async fn send_media(
        &self,
        chat: &str,
        file_path: &str,
        caption: &str,
    ) -> Result<Message, AgentError> {
        for component in std::path::Path::new(file_path).components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(AgentError::invalid_input(
                    "file_path must not contain '..' components",
                ));
            }
        }

        let (media_type, mime) = detect_media_type(file_path);

        let media_id = self.upload_media(file_path, mime).await?;

        let url = self.url("/messages");

        let supports_caption = matches!(media_type, "image" | "video" | "document");
        let media_obj = if caption.is_empty() || !supports_caption {
            serde_json::json!({ "id": media_id })
        } else {
            serde_json::json!({ "id": media_id, "caption": caption })
        };

        let mut body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": chat,
            "type": media_type,
        });
        body[media_type] = media_obj;

        let resp = self.api_post_json(&url, &body).await?;

        let parsed: SendMessageResponse = serde_json::from_value(resp)
            .map_err(|e| AgentError::api(format!("whatsapp parse send response: {e}")))?;

        let msg_id = parsed
            .messages
            .into_iter()
            .next()
            .map(|m| m.id)
            .ok_or_else(|| AgentError::api("whatsapp: no message id in response"))?;

        Ok(Message {
            id: msg_id,
            platform: Platform::WhatsApp,
            channel_id: chat.to_string(),
            sender: self.phone_number_id.clone(),
            text: caption.to_string(),
            timestamp: now_ts(),
            has_attachment: true,
            reply_to: None,
            meta: MessageMeta {
                media_type: Some(media_type.to_string()),
                ..MessageMeta::default()
            },
        })
    }
}
