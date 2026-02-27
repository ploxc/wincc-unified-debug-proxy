# WinCC Unified Debug Proxy

Debug WinCC Unified JavaScript scripts in VS Code with automatic reconnection.

**[Download the latest release](https://github.com/ploxc/wincc-unified-debug-proxy/releases/latest)**

**[Read the documentation](https://ploxc.com/tools/debug-proxy/docs)**

## The problem

- **Targets change constantly** — every screen reload, navigation, or runtime restart creates a new debug target. Your debugger disconnects and you have to manually reattach.
- **`chrome://inspect` is slow** — the default workflow opens a new DevTools window on every reload, losing breakpoints and console history each time.
- **Dynamics and Events are separate contexts** — WinCC runs property animations and event handlers in separate V8 contexts, requiring two independent debug sessions.


## The solution

The proxy sits between VS Code and the WinCC debug server. It runs two local WebSocket servers — one for Dynamics (port 9230) and one for Events (port 9231) — forwarding Chrome DevTools Protocol messages to WinCC on port 9222.

It polls the WinCC `/json` endpoint to detect target changes. When a target changes, it tears down connections and restarts, forcing VS Code to automatically reconnect via `"restart": true` in the launch config.

## Quick start

**1. Generate VS Code debug configuration:**

```
./wincc-unified-debug-proxy.exe init
```

**2. Start the proxy:**

```
./wincc-unified-debug-proxy.exe run
```

**3. Debug:** open Run and Debug (`Ctrl+Shift+D`), pick **WinCC:Dynamics**, **WinCC:Events**, or **WinCC:All**, and press `F5`.

## Features

- **Auto reconnect** — detects target changes and forces VS Code to reconnect automatically
- **Separate ports** — Dynamics (`:9230`) and Events (`:9231`) on independent proxy ports
- **Auto session selection** — picks the most recent active debug target when multiple exist
- **Script dump** — extract all runtime scripts to disk with `--dump` for backup, diffing, or AI-assisted review
- **ESLint + IntelliSense** — type definitions and linting setup for dumped scripts (v17–v21)
- **Remote debugging** — generate netsh port forwarding scripts with `generate`
- **Path shortening** — rewrites verbose script URLs to readable paths (e.g. `HMI_Screen/Pump_Symbol/Events.js`)

## CLI reference

### `run` (default)

Starts the proxy. This is the default when no command is specified.

```
./wincc-unified-debug-proxy.exe run [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-t, --target-host` | `localhost` | WinCC host address |
| `-p, --target-port` | `9222` | WinCC debug port |
| `-d, --dynamics-port` | `9230` | Local Dynamics proxy port |
| `-e, --events-port` | `9231` | Local Events proxy port |
| `-i, --poll-interval` | `1` | Target polling interval (seconds) |
| `-l, --long-paths` | off | Show full script paths |
| `-v, --verbose` | off | Verbose logging |
| `-V, --very-verbose` | off | Per-message logging |
| `--dump <dir>` | off | Dump runtime scripts to directory |

### `init`

Creates `.vscode/launch.json` with debug configurations for Dynamics and Events.

```
./wincc-unified-debug-proxy.exe init [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output` | `.` | Output directory |
| `-d, --dynamics-port` | `9230` | Dynamics port in launch.json |
| `-e, --events-port` | `9231` | Events port in launch.json |

### `generate`

Creates `.bat` scripts for netsh port forwarding and firewall rules on the WinCC machine.

```
./wincc-unified-debug-proxy.exe generate --address <IP> [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-a, --address` | *required* | WinCC machine IP |
| `-p, --port` | `9222` | WinCC debug port |
| `-o, --output` | `.` | Output directory |

## Documentation

Full docs at [ploxc.com/tools/debug-proxy/docs](https://ploxc.com/tools/debug-proxy/docs).

## Build from source

Requires [Rust](https://www.rust-lang.org/tools/install).

```
git clone https://github.com/ploxc/wincc-unified-debug-proxy.git
cd wincc-unified-debug-proxy
cargo build --release
```

The binary will be at `target/release/wincc-unified-debug-proxy.exe`.

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.
