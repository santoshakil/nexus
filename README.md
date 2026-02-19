# Nexus

MCP server that gives AI agents access to messaging platforms. One binary, five platforms.

```
Telegram · Gmail · WhatsApp · Slack · Discord
```

Nexus implements the [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) over stdio, exposing **48 tools** that let AI agents read, send, search, and manage messages across platforms through a unified interface.

Built in pure Rust. No MCP SDK dependencies — hand-rolled JSON-RPC 2.0. Single binary, ~4MB release.

## Why Nexus?

AI agents need to communicate. They need to read your Telegram messages, send emails, search Slack history, and manage Discord channels — but each platform has its own API, auth flow, and data model.

Nexus unifies all of this behind MCP. Connect it to [Claude Code](https://claude.ai/code), [Claude Desktop](https://claude.ai/download), or any MCP client, and your agent gets instant access to your messaging world.

**What agents can do with Nexus:**
- Read and send messages across all platforms
- Search chat history (find that message from months ago)
- Manage emails (archive, label, star, draft, send with attachments)
- Forward, pin, react to, edit, and delete Telegram messages
- Create Slack channels, set topics, manage reactions
- List Discord guilds, create threads, pin messages
- Download media and attachments from any platform

## Quick Start

### 1. Install

**From source:**
```bash
git clone https://github.com/santoshakil/nexus.git
cd nexus
cargo build --release
cp target/release/nexus ~/.local/bin/
```

**Prerequisites:**
- Rust 1.75+
- TDLib (`libtdjson.so`) — only needed for Telegram support. See [TDLib build instructions](https://tdlib.github.io/td/build.html).

### 2. Configure

Copy the example env file and fill in the platforms you want:

```bash
cp .env.example .env
```

You only need to configure the platforms you'll use. Nexus silently skips unconfigured platforms.

```bash
# Telegram (get from https://my.telegram.org)
TELEGRAM_API_ID=12345678
TELEGRAM_API_HASH=abcdef1234567890abcdef1234567890

# Gmail (generate App Password at https://myaccount.google.com/apppasswords)
GMAIL_ADDRESS=you@gmail.com
GMAIL_APP_PASSWORD=xxxx xxxx xxxx xxxx

# WhatsApp Business Cloud API (https://developers.facebook.com)
WHATSAPP_ACCESS_TOKEN=your_token
WHATSAPP_PHONE_NUMBER_ID=your_phone_id

# Slack (create app at https://api.slack.com/apps)
SLACK_BOT_TOKEN=xoxb-your-bot-token

# Discord (create app at https://discord.com/developers/applications)
DISCORD_BOT_TOKEN=your_bot_token
```

### 3. Authenticate Telegram (one-time)

Telegram requires an interactive login the first time:

```bash
nexus auth telegram
```

This prompts for your phone number, SMS code, and optional 2FA password. The session is saved to `~/.nexus/tdlib/` and reused automatically.

### 4. Connect to Claude Code

Add to your Claude Code MCP config (`~/.claude/claude_code_config.json`):

```json
{
  "mcpServers": {
    "nexus": {
      "command": "nexus",
      "args": ["mcp"],
      "env": {
        "TELEGRAM_API_ID": "12345678",
        "TELEGRAM_API_HASH": "abcdef1234567890abcdef1234567890",
        "GMAIL_ADDRESS": "you@gmail.com",
        "GMAIL_APP_PASSWORD": "xxxx xxxx xxxx xxxx"
      }
    }
  }
}
```

Or for Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "nexus": {
      "command": "/path/to/nexus",
      "args": ["mcp"],
      "env": {
        "TELEGRAM_API_ID": "12345678",
        "TELEGRAM_API_HASH": "abcdef1234567890abcdef1234567890"
      }
    }
  }
}
```

## Tools

Nexus exposes tools dynamically — only tools for configured platforms appear in `tools/list`.

### Universal Tools (all platforms)

| Tool | Description |
|------|-------------|
| `list_platforms` | List connected platforms |
| `get_profile` | Get authenticated user profile |
| `list_channels` | List chats, folders, channels |
| `read_messages` | Read messages with pagination |
| `send_message` | Send a message (with optional reply) |
| `search` | Search messages across a platform |

### Telegram Tools (13)

| Tool | Description |
|------|-------------|
| `telegram_search_chat` | Search within a specific chat |
| `telegram_get_message` | Get a single message with full metadata |
| `telegram_get_chat_info` | Chat details, member count, description |
| `telegram_get_chat_members` | List members with roles |
| `telegram_send_media` | Send photo/video/document |
| `telegram_download_media` | Download media to local file |
| `telegram_forward_message` | Forward between chats |
| `telegram_edit_message` | Edit your sent messages |
| `telegram_delete_messages` | Delete messages |
| `telegram_pin_message` | Pin a message |
| `telegram_unpin_message` | Unpin a message |
| `telegram_mark_read` | Mark messages as read |
| `telegram_react` | Add emoji reaction |

### Gmail Tools (13)

| Tool | Description |
|------|-------------|
| `gmail_send_email` | Send with CC, BCC, attachments |
| `gmail_create_draft` | Create draft without sending |
| `gmail_archive` | Remove from inbox |
| `gmail_trash` | Move to trash |
| `gmail_move_to` | Move to any folder |
| `gmail_list_labels` | List all labels/folders |
| `gmail_add_label` | Add label to message |
| `gmail_remove_label` | Remove label from message |
| `gmail_mark_read` | Mark as read |
| `gmail_mark_unread` | Mark as unread |
| `gmail_star` | Star a message |
| `gmail_unstar` | Unstar a message |
| `gmail_get_attachment` | Download attachment to file |

### Slack Tools (9)

| Tool | Description |
|------|-------------|
| `slack_set_status` | Set status message and emoji |
| `slack_create_channel` | Create a new channel |
| `slack_invite_to_channel` | Invite user to channel |
| `slack_set_topic` | Set channel topic |
| `slack_add_reaction` | React to a message |
| `slack_remove_reaction` | Remove reaction |
| `slack_upload_file` | Upload file to channels |
| `slack_list_users` | List workspace users |
| `slack_get_user_info` | Get user profile details |

### Discord Tools (6)

| Tool | Description |
|------|-------------|
| `discord_list_guilds` | List servers the bot is in |
| `discord_list_guild_channels` | List channels in a server |
| `discord_create_thread` | Create a thread |
| `discord_add_reaction` | React to a message |
| `discord_remove_reaction` | Remove reaction |
| `discord_pin_message` | Pin a message |

### WhatsApp Tools (1)

| Tool | Description |
|------|-------------|
| `whatsapp_send_media` | Send media file to a number |

> WhatsApp Business Cloud API is limited to sending only. Message history is not available through the API.

## Format System

Every data-returning tool accepts an optional `format` parameter:

| Format | Behavior | Use Case |
|--------|----------|----------|
| `compact` (default) | One-liner per item, text truncated to 200 chars | Browsing, scanning, token-efficient |
| `expanded` | One-liner per item, full untruncated text | Reading full messages |
| `full` | Complete JSON with all fields | When you need exact field values |

**Examples:**

Compact (default):
```
[Jan 15 10:30] John Doe (@john): Hey, I was thinking about the architecture for the new...  (id:123456)
```

Expanded:
```
[Jan 15 10:30] John Doe (@john): Hey, I was thinking about the architecture for the new messaging service. We should probably use a hexagonal pattern with ports and adapters. What do you think? (id:123456)
```

Full:
```json
{"id":"123456","platform":"telegram","channel_id":"-100123","sender":"John Doe","text":"Hey, I was thinking about...","timestamp":1705312200,...}
```

## Architecture

Hexagonal (ports-and-adapters) with 9 workspace crates:

```
app-mcp ──→ mod-messaging ──→ core-domain ──→ core-error
                  ↑                 ↑
        infra-tdlib          (port traits)
        infra-google
        infra-whatsapp
        infra-slack
        infra-discord
```

| Layer | Crate | Purpose |
|-------|-------|---------|
| **Core** | `core-error` | `AgentError` enum with suggestions and retryable flags |
| **Core** | `core-domain` | Shared entities (`Message`, `Channel`, `Profile`, etc.) and port traits (`MessagingPort`, `TelegramExt`, `GmailExt`, etc.) |
| **Module** | `mod-messaging` | `AgentService` (platform registry + routing) and `Format` engine |
| **Infra** | `infra-tdlib` | Telegram via TDLib FFI (4 C functions, dedicated receive thread) |
| **Infra** | `infra-google` | Gmail via IMAP + SMTP (connection pooling, MIME decoding) |
| **Infra** | `infra-whatsapp` | WhatsApp Business Cloud API via HTTP |
| **Infra** | `infra-slack` | Slack Web API via HTTP |
| **Infra** | `infra-discord` | Discord Bot API v10 via HTTP |
| **App** | `app-mcp` | Binary: CLI, MCP server (stdio JSON-RPC), tool definitions |

### MCP Protocol

Nexus uses **newline-delimited JSON-RPC 2.0 on stdio** (not Content-Length headers). stdout is exclusively for JSON-RPC responses. All logging goes to stderr via `tracing`.

### Error Handling

Every error includes context to help the AI agent recover:

```json
{
  "isError": true,
  "content": [{"type": "text", "text": "[auth] Telegram session expired | try: re-run `nexus auth telegram` | retryable: false"}]
}
```

Errors include a suggestion (actionable fix) and whether the operation is retryable.

## Platform Setup Guides

### Telegram

1. Go to [my.telegram.org](https://my.telegram.org) and create an application
2. Note your `API_ID` and `API_HASH`
3. Set environment variables:
   ```bash
   export TELEGRAM_API_ID=12345678
   export TELEGRAM_API_HASH=abcdef1234567890abcdef1234567890
   ```
4. Run one-time authentication:
   ```bash
   nexus auth telegram
   ```
   Enter your phone number, SMS code, and 2FA password if enabled.
5. The session is saved to `~/.nexus/tdlib/` — you won't need to authenticate again unless you revoke the session.

**Note:** Telegram requires [TDLib](https://github.com/tdlib/td) (`libtdjson.so`) installed at `/usr/local/lib`. See the [TDLib build guide](https://tdlib.github.io/td/build.html) for your platform. Set `TDLIB_DIR` env var if installed elsewhere.

### Gmail

1. Enable 2-Step Verification on your Google account
2. Generate an App Password at [myaccount.google.com/apppasswords](https://myaccount.google.com/apppasswords)
   - Select "Mail" as the app
   - Copy the 16-character password
3. Set environment variables:
   ```bash
   export GMAIL_ADDRESS=you@gmail.com
   export GMAIL_APP_PASSWORD="xxxx xxxx xxxx xxxx"
   ```

**Gmail search syntax:** The `search` tool accepts IMAP criteria:
- Simple text: `"meeting notes"` (searches body)
- From sender: `FROM sender@example.com`
- By subject: `SUBJECT "quarterly report"`
- By date: `SINCE 01-Jan-2025`
- Unread only: `UNSEEN`
- Combine: `FROM boss@company.com SINCE 01-Feb-2025 UNSEEN`

### WhatsApp

1. Create a [Meta Developer](https://developers.facebook.com/) account
2. Create a new app with WhatsApp product
3. Get your access token and phone number ID from the WhatsApp API dashboard
4. Set environment variables:
   ```bash
   export WHATSAPP_ACCESS_TOKEN=your_token
   export WHATSAPP_PHONE_NUMBER_ID=your_phone_id
   ```

**Limitations:** WhatsApp Business Cloud API only supports sending messages. You cannot read message history or search messages through the API.

### Slack

1. Go to [api.slack.com/apps](https://api.slack.com/apps) and create a new app
2. Under **OAuth & Permissions**, add these bot token scopes:
   - `channels:history`, `channels:read`, `channels:write`
   - `chat:write`
   - `groups:history`, `groups:read`
   - `im:history`, `im:read`
   - `mpim:history`, `mpim:read`
   - `reactions:read`, `reactions:write`
   - `users:read`
   - `files:write` (for file uploads)
3. Install the app to your workspace
4. Copy the Bot User OAuth Token (`xoxb-...`)
5. Set environment variable:
   ```bash
   export SLACK_BOT_TOKEN=xoxb-your-bot-token
   ```

### Discord

1. Go to [discord.com/developers/applications](https://discord.com/developers/applications)
2. Create a new application
3. Go to **Bot** tab and create a bot
4. Enable these Privileged Gateway Intents: **Message Content Intent**
5. Copy the bot token
6. Invite the bot to your server using OAuth2 URL Generator with `bot` scope and these permissions:
   - Read Messages/View Channels
   - Send Messages
   - Manage Messages (for pin/delete)
   - Add Reactions
   - Read Message History
   - Create Public Threads
7. Set environment variable:
   ```bash
   export DISCORD_BOT_TOKEN=your_bot_token
   ```

## CLI Reference

```bash
# Start the MCP server (default command)
nexus mcp

# Authenticate with Telegram (interactive, one-time)
nexus auth telegram

# Show help and environment variables
nexus help
```

### Environment Variables

| Variable | Platform | Description |
|----------|----------|-------------|
| `TELEGRAM_API_ID` | Telegram | API ID from my.telegram.org |
| `TELEGRAM_API_HASH` | Telegram | API hash from my.telegram.org |
| `GMAIL_ADDRESS` | Gmail | Your Gmail address |
| `GMAIL_APP_PASSWORD` | Gmail | App Password (not your regular password) |
| `WHATSAPP_ACCESS_TOKEN` | WhatsApp | Business Cloud API token |
| `WHATSAPP_PHONE_NUMBER_ID` | WhatsApp | Sender phone number ID |
| `SLACK_BOT_TOKEN` | Slack | Bot User OAuth Token (`xoxb-...`) |
| `DISCORD_BOT_TOKEN` | Discord | Bot token |
| `NEXUS_DATA_DIR` | All | Data directory (default: `~/.nexus`) |
| `RUST_LOG` | All | Log level (default: `nexus=info`) |

## Building from Source

```bash
# Debug build
cargo build --workspace

# Release build (optimized, ~4MB)
cargo build --release

# Check/lint
cargo check --workspace
cargo clippy --workspace -- -D warnings

# Run protocol tests (needs debug build)
cargo build --workspace
bash tests/mcp_protocol_test.sh
```

### Build Configuration

The release profile is optimized for small binary size:
- LTO: fat (link-time optimization)
- Codegen units: 1
- Strip: enabled
- Panic: abort

### Workspace Lints

These are enforced across all crates:
- `clippy::unwrap_used = "deny"` — use `?` or `match`
- `clippy::expect_used = "deny"` — use `?` or `match`
- `unused_must_use = "deny"`
- `unsafe_code = "warn"` (only allowed in `infra-tdlib` for FFI)

## Adding a New Platform

1. Create `crates/infra-{name}/` with `Cargo.toml` and `src/adapter.rs`
2. Add a variant to `Platform` enum in `core-domain/src/entities.rs`
3. Implement the `MessagingPort` trait (6 methods)
4. Optionally add an extension trait in `core-domain/src/ports.rs`
5. Register the adapter in `app-mcp/src/main.rs`
6. Add tool definitions in `app-mcp/src/mcp/tools.rs`
7. Wire dispatch in `app-mcp/src/mcp/server.rs`

## Adding a New Tool

1. Add the method to the relevant extension trait in `core-domain/src/ports.rs`
2. Implement it in the adapter (e.g., `infra-tdlib/src/adapter.rs`)
3. Add a `ToolDef` in `app-mcp/src/mcp/tools.rs`
4. Add a dispatch arm in `app-mcp/src/mcp/server.rs`

## Contributing

Contributions are welcome. Please:

1. Fork the repo and create a feature branch
2. Ensure `cargo clippy --workspace -- -D warnings` passes with no warnings
3. Ensure `cargo check --workspace` compiles cleanly
4. Run `bash tests/mcp_protocol_test.sh` (needs a debug build)
5. Keep commits atomic and descriptive
6. Open a pull request with a clear description

## License

MIT — see [LICENSE](LICENSE) for details.
