# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Nexus

MCP (Model Context Protocol) server that gives AI agents access to messaging platforms — Telegram, Gmail, WhatsApp, Slack, Discord. Pure Rust, no SDK dependencies for MCP (hand-rolled JSON-RPC 2.0 over stdio). Single binary (~3.9MB release).

## Commands

```bash
# Check/lint (safe to run anytime)
cargo check --workspace
cargo clippy --workspace -- -D warnings

# Run MCP server (requires platform env vars)
nexus mcp

# Authenticate Telegram (interactive, one-time)
nexus auth telegram

# Protocol smoke test (requires debug binary)
bash tests/mcp_protocol_test.sh

# Build (ask before running)
cargo build --workspace
cargo build --release
```

No unit test suite exists yet — only `tests/mcp_protocol_test.sh` (shell-based MCP protocol validation).

## Architecture

Hexagonal (ports-and-adapters) with 9 workspace crates:

```
app-mcp → mod-messaging → core-domain → core-error
              ↑                ↑
     infra-{tdlib,google,whatsapp,slack,discord}
```

### Core layer
- **core-error** (`nexus-error`): `AgentError` enum (Auth, Api, Network, Session, NotFound, InvalidInput, PlatformNotAvailable, NotImplemented, Internal) + `ErrorResponse` with suggestion/retryable for MCP error display
- **core-domain** (`nexus-domain`): Shared entities (Platform, Channel, Message, MessageMeta, Profile, ChatInfo, ChatMember, Paginated\<T\>) and port traits — `MessagingPort` (6 universal methods) + extension traits per platform (`TelegramExt`, `GmailExt`, `WhatsAppExt`, `SlackExt`, `DiscordExt`)

### Module layer
- **mod-messaging** (`nexus-messaging`): `AgentService` — HashMap\<Platform, Arc\<dyn MessagingPort\>\> registry that routes calls to adapters. `Format` engine (compact one-liner vs full JSON) in `format.rs`

### Infrastructure layer (one crate per platform)
- **infra-tdlib** (`nexus-tdlib`): TDLib FFI (4 C functions via `ffi.rs`), `TdClient` (receive loop on dedicated OS thread, request/response correlation via `@extra` + oneshot channels), `TdlibAdapter` implements `MessagingPort + TelegramExt`, chat name→id resolution cache (Arc\<RwLock\<HashMap\>\> with 5-min TTL). Links `libtdjson.so`. Build script in `build.rs` links the lib.
- **infra-google** (`nexus-google`): Gmail via IMAP (sync `imap` crate + `spawn_blocking`) and SMTP (async `lettre`). Connection pooling via `Arc<Mutex<Option<ImapSession>>>`. MIME decoding via `mailparse`.
- **infra-whatsapp** (`nexus-whatsapp`): WhatsApp Business Cloud API via `reqwest`. Limited: send text/media only, no read/search.
- **infra-slack** (`nexus-slack`): Slack Web API via `reqwest`.
- **infra-discord** (`nexus-discord`): Discord Bot API v10 via `reqwest`.

### Application layer
- **app-mcp** (`nexus`): Binary crate. `main.rs` loads configs from env vars, creates adapters, registers them with `AgentService`. `mcp/` module: `server.rs` (stdio JSON-RPC loop, dispatches to tools), `tools.rs` (48 tool definitions with JSON schemas, dynamically filtered by connected platforms), `types.rs` (RpcMessage, RpcResponse, ToolDef, ToolResult, CallToolParams).

## Key patterns

**Adding a new MCP tool:**
1. Add method to the relevant extension trait in `core-domain/src/ports.rs`
2. Implement in the adapter (e.g. `infra-tdlib/src/adapter.rs`)
3. Add `ToolDef` in `app-mcp/src/mcp/tools.rs` (in the platform's tool function)
4. Add dispatch arm in `app-mcp/src/mcp/server.rs` `dispatch_tool()` match

**Adding a new platform:**
1. Create `infra-{name}` crate implementing `MessagingPort` (+ optional extension trait in `core-domain/src/ports.rs`)
2. Register adapter in `app-mcp/src/main.rs` `run_mcp_server()`
3. Add platform variant to `Platform` enum in `core-domain/src/entities.rs`
4. Add tool definitions in `app-mcp/src/mcp/tools.rs`
5. Wire dispatch in `server.rs`

**MCP protocol:** Newline-delimited JSON-RPC 2.0 on stdio (NOT Content-Length headers). stdout is exclusively for JSON-RPC responses. All logging goes to stderr via `tracing`.

**Format system:** Every data-returning tool accepts optional `format` param. Compact (default) = one-liners with truncated text (200 chars) for token efficiency. Expanded = one-liners with full untruncated text. Full = raw JSON serialization.

**Error flow:** Adapter returns `AgentError` → `ErrorResponse::from()` → `to_compact()` → `ToolResult::failure()`.

## Workspace lints (enforced)

- `clippy::unwrap_used = "deny"` and `clippy::expect_used = "deny"` — use `?` or `match` everywhere
- `unused_must_use = "deny"`
- `unsafe_code = "warn"` (only allowed in `infra-tdlib` via `#![allow(unsafe_code)]`)

## Environment variables

```
TELEGRAM_API_ID / TELEGRAM_API_HASH    Telegram (also needs one-time `nexus auth telegram`)
GMAIL_ADDRESS / GMAIL_APP_PASSWORD     Gmail (app password, not OAuth)
WHATSAPP_ACCESS_TOKEN / WHATSAPP_PHONE_NUMBER_ID  WhatsApp Business Cloud API
SLACK_BOT_TOKEN                        Slack (xoxb-...)
DISCORD_BOT_TOKEN                      Discord Bot
NEXUS_DATA_DIR                         Data dir (default: ~/.nexus)
RUST_LOG                               Log level (default: nexus=info)
```

Platforms without env vars are silently skipped — tools/list only shows tools for connected platforms.

## Prerequisites

- TDLib (`libtdjson.so`) at /usr/local/lib (for Telegram support)
- Rust 1.75+
- Release binary installed at `~/.local/bin/nexus`
