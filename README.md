# barkeep

An **MCP-native command palette for Linux**. One hotkey, one input box: launch apps,
ask an AI, run MCP tools. Rust + iced + wgpu.

Born from an audit of [PossumXI/AI_UI](https://github.com/PossumXI/AI_UI): the idea was
sound, the execution stalled. barkeep keeps the two things that worked (the global
hotkey pattern, the Claude client approach) and drops the rest. Everything here is
written to be small, tested, and honest about what works.

## Why

- Raycast doesn't exist on Linux. Launchers that do (uLauncher, Albert, rofi) treat
  AI as a bolt-on plugin, and none speak MCP natively.
- MCP is the point, not a feature: every configured stdio server's tools show up in
  the palette and run in-process.
- Claude when you have a key, local Ollama when you don't. No lock-in either way.

## Usage

| Keys | Action |
|---|---|
| `Ctrl+Space` | summon (focus + reset) |
| `Enter` | act on top result / submit |
| `Esc` | clear; Esc again quits |

| Query | Mode |
|---|---|
| `firefox` | fuzzy app search, Enter launches |
| `? explain this error` | streams an AI answer (Claude, Ollama fallback) |
| `#echo {"text":"hi"}` | MCP tool search and invoke |

App entries come from freedesktop `.desktop` files; Exec field codes (`%f`, `%u` ...) are stripped before launch.

## Config

`~/.config/barkeep/config.toml` (all optional):

```toml
[ai]
model = "claude-sonnet-4-5-20250929"
ollama_model = "qwen3.5:0.8b"
ollama_url = "http://localhost:11434"
# anthropic_key = "sk-ant-..."   # env var ANTHROPIC_API_KEY takes priority

[[mcp.server]]
name = "echo"
command = "python3"
args = ["/absolute/path/to/echo_mcp_server.py"]
```

## Build

```bash
cargo run            # debug
cargo test           # unit + MCP fixture integration tests
cargo build --release
```

Linux only for now. Wayland layer-shell overlay (proper hide/show instead of
always-on-top) is the next milestone.

## Status

v0.1 - prove the loop: palette, hotkey, app launch, streaming AI, stdio MCP client.
See `tests/` for what is actually verified.

## License

MIT
