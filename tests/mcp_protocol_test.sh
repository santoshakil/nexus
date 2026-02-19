#!/bin/bash
# Manual MCP protocol test — validates JSON-RPC responses
# Usage: bash tests/mcp_protocol_test.sh

NEXUS="${1:-./target/debug/nexus}"

if [ ! -x "$NEXUS" ]; then
    echo "Binary not found: $NEXUS"
    echo "Run: cargo build --workspace"
    exit 1
fi

PASS=0
FAIL=0

check() {
    local desc="$1"
    local input="$2"
    local expected="$3"
    local line="${4:-1}"

    local output
    output=$(echo "$input" | RUST_LOG=off "$NEXUS" mcp 2>/dev/null | sed -n "${line}p")

    if echo "$output" | grep -qF "$expected"; then
        echo "PASS: $desc"
        PASS=$((PASS + 1))
    else
        echo "FAIL: $desc"
        echo "  Input:    $input"
        echo "  Expected: $expected"
        echo "  Got:      $output"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== Nexus MCP Protocol Tests ==="
echo

check "initialize returns protocolVersion" \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}' \
    'protocolVersion'

check "tools/list returns get_profile" \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    'get_profile' 2

check "tools/list returns send_message" \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    'send_message' 2

check "ping returns empty result" \
    '{"jsonrpc":"2.0","id":1,"method":"ping"}' \
    '{}'

check "unknown method returns error" \
    '{"jsonrpc":"2.0","id":1,"method":"nonexistent"}' \
    'unknown method'

check "parse error on invalid JSON" \
    'not json at all' \
    'parse error'

echo
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
