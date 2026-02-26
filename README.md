# WinCC Unified Debug Proxy

Debug WinCC Unified JavaScript scripts in VS Code. The proxy sits between VS Code and the WinCC runtime debug server, handling automatic reconnection when scripts reload.

## Quick Start

1. Download `wincc-unified-debug-proxy.exe` from the [latest release](https://github.com/ploxc/wincc-unified-debug-proxy/releases/latest) and place it somewhere on your PATH (or in your project folder).

2. Create a VS Code debug configuration:
   ```
   wincc-unified-debug-proxy init
   ```

3. Start the proxy:
   ```
   wincc-unified-debug-proxy
   ```

4. In VS Code, open the **Run and Debug** panel and launch **WinCC:Dynamics**, **WinCC:Events**, or **WinCC:All**.

## Commands

### `run` (default)

Starts the proxy. This is the default when no command is specified.

| Flag | Long | Default | Description |
|------|------|---------|-------------|
| `-t` | `--target-host` | `localhost` | Target WinCC host address |
| `-p` | `--target-port` | `9222` | Target WinCC debug port |
| `-d` | `--dynamics-port` | `9230` | Local port for Dynamics proxy |
| `-e` | `--events-port` | `9231` | Local port for Events proxy |
| `-i` | `--poll-interval` | `1` | Poll interval in seconds |
| `-v` | `--verbose` | off | Enable verbose logging |
| `-V` | `--very-verbose` | off | Enable very verbose logging |
| `-l` | `--long-paths` | off | Show full script paths instead of shortened ones |
| | `--dump <DIR>` | off | Continuously dump runtime scripts to local files |

### `init`

Creates `.vscode/launch.json` with debug configurations for Dynamics and Events.

| Flag | Long | Default | Description |
|------|------|---------|-------------|
| `-o` | `--output` | `.` | Output directory |
| `-d` | `--dynamics-port` | `9230` | Dynamics port (used in launch.json) |
| `-e` | `--events-port` | `9231` | Events port (used in launch.json) |

### `generate`

Creates `.bat` scripts that configure netsh port forwarding and firewall rules on the WinCC machine, needed for remote debugging.

| Flag | Long | Default | Description |
|------|------|---------|-------------|
| `-a` | `--address` | *required* | IP address of the WinCC machine |
| `-p` | `--port` | `9222` | WinCC debug port |
| `-o` | `--output` | `.` | Output directory for .bat files |

## Remote Debugging

To debug a WinCC runtime on a different machine:

1. Generate the setup scripts:
   ```
   wincc-unified-debug-proxy generate -a 192.168.1.100
   ```

2. Copy the generated `wincc-debug-setup-*.bat` to the WinCC machine and run it **as Administrator**. This creates netsh port forwarding and firewall rules so the debug port is accessible from the network.

3. Start the proxy pointing at the remote host:
   ```
   wincc-unified-debug-proxy run -t 192.168.1.100
   ```

4. After a Windows restart on the WinCC machine, re-run `wincc-debug-restart-*.bat` to restore the port proxy rule. To remove everything, run `wincc-debug-cleanup-*.bat`.

## Script Dumping & TypeDefs

Use `--dump` to continuously save runtime scripts to disk as they load:

```
wincc-unified-debug-proxy run --dump ./output
```

When `--dump` is used, the proxy prompts for your WinCC version (v17-v21) and writes TypeScript definitions, ESLint config, and jsconfig.json into the dump directory. This gives you IntelliSense and linting for WinCC scripts in VS Code. Requires [Node.js](https://nodejs.org/) for ESLint.

## How It Works

The proxy runs two local WebSocket servers — one for Dynamics (port 9230) and one for Events (port 9231). Both forward Chrome DevTools Protocol (CDP) messages to the WinCC debug server on port 9222.

It polls the WinCC `/json` endpoint to detect when script targets change (which happens on every screen navigation). When a change is detected, it tears down the current connections and restarts, forcing VS Code to automatically reconnect via its `"restart": true` launch config.

Script URLs are shortened for readability (e.g. `/screen_modules/Screen_Content/HMI_RT_1::HMI_Screen/faceplate_modules/CM_Freq/Events.js` becomes `HMI_Screen/CM_Freq/Events.js`). Use `-l` to show full paths.

## Building from Source

Requires [Rust](https://www.rust-lang.org/tools/install).

```
cargo build --release
```

The binary will be at `target/release/wincc-unified-debug-proxy.exe`.
