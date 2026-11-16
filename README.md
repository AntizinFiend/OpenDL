# OpenDL

OpenDL is a high-performance download manager and accelerator written in Rust. It combines a dynamic segment allocation engine with an aria2c sidecar process, Windows Job Object process confinement, an SQLite persistence layer, a Tauri v2 desktop user interface, a command-line interface, and a Manifest V3 browser extension.

---

## Architectural Overview

OpenDL is structured as a Cargo workspace consisting of five modular Rust crates, technical specification documents, and a browser companion extension:

- **opendl-engine**: Handles download mechanics, target probing, range validation, direct file host stream resolution, dynamic chunk splitting, and aria2c JSON-RPC process control.
- **opendl-core**: Provides data persistence using SQLite in WAL mode, multi-queue state machines, power actions, pure-Rust bencode parsing, SHA-1 torrent info-hash calculation, and Windows Named Pipe IPC.
- **opendl-desktop**: The primary graphical desktop application built with Tauri v2 and Microsoft Webview2, providing virtual scrolling, dynamic segment canvas visualization, speed graphs, queue management, and a BitTorrent telemetry dock.
- **opendl-native-host**: A native messaging executable communicating with Chromium and Firefox browsers via length-prefixed standard I/O framing, forwarding intercept payloads to the core application over Named Pipes.
- **opendl-cli**: A command-line client supporting multi-connection downloads, interactive terminal progress bars, and Model Context Protocol (MCP) server capabilities.
- **extension**: A Manifest V3 WebExtension supporting non-blocking in-page download capture, root-domain session cookie harvesting, and magnet link interception.

---

## Key Capabilities

### 1. Hybrid Download Engine and Process Confinement
- Direct random-access writes (`seek_write`) preallocate file space on disk, avoiding post-download file reassembly or concatenation delays.
- Dynamic segment splitting divides large incomplete file ranges when worker threads finish early.
- External aria2c engine processes are spawned with Windows Job Objects (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), ensuring that child processes terminate cleanly if the parent process exits or crashes.
- Configured with realistic browser User-Agent headers, automated gzip decompression, and retry policies for maximum server compatibility.

### 2. Direct Stream Resolver for File Hosts
- Built-in resolvers automatically analyze and extract direct streaming endpoints from file hosts such as Mediafire, Google Drive, and Dropbox.
- Captures dynamic download keys, virus confirmation tokens, and transient session cookies without requiring external browser automation.
- Distinguishes between direct binary streams and HTML error pages, preventing corrupt small-file downloads.

### 3. BitTorrent and Magnet Support
- Implements a pure-Rust bencode parser and SHA-1 info-hash calculation with zero external dependencies.
- Interactive file selection dialog allows choosing specific files within a multi-file torrent, setting file priority levels, or excluding unwanted files.
- Configurable seeding ratio controls with live tracking of upload speed, uploaded bytes, and seed state.
- Three-column transfer dock and three-column metadata dock modeled after qBittorrent telemetry displays: active time, session upload and download, average speeds, swarm health, peer counts, piece dimensions, hash strings, and comments.

### 4. Advanced Queue Management and Scheduling
- Multi-queue support with custom concurrency limits, speed restrictions, and automated schedules.
- Interactive drag-and-drop table row reordering. Promoting a queued download past the concurrency threshold automatically starts it and suspends the displaced download to preserve concurrency rules.
- Queue-level completion triggers execute system power commands, including shutdown, sleep, hibernation, or application exit.

### 5. Header and Session Persistence
- Persists request headers, referer URLs, and session cookies in SQLite.
- Pausing, resuming, or moving tasks between queues preserves the original authentication state and headers across application restarts.

### 6. Desktop User Interface
- Lightweight Tauri v2 desktop application using hardware-accelerated Webview2.
- Virtual scrolling table maintaining fixed column alignments across all category and queue views.
- Real-time HTML5 Canvas visualizer rendering multi-lane segment download progress.
- Built-in theme selector (Dark, Light, Amber Day, Blueprint, and System) with configurable UI settings and drive space monitoring.

---

## Repository Structure

```
Idm/
|-- crates/
|   |-- opendl-cli/
|   |   |-- src/
|   |   |   |-- main.rs
|   |   |   `-- mcp.rs
|   |   `-- Cargo.toml
|   |-- opendl-core/
|   |   |-- src/
|   |   |   |-- db.rs
|   |   |   |-- ipc.rs
|   |   |   |-- lib.rs
|   |   |   |-- manager.rs
|   |   |   |-- power.rs
|   |   |   `-- torrent.rs
|   |   |-- tests/
|   |   |   `-- core_tests.rs
|   |   `-- Cargo.toml
|   |-- opendl-desktop/
|   |   |-- bin/
|   |   |   `-- aria2c.exe
|   |   |-- icons/
|   |   |-- src/
|   |   |   `-- main.rs
|   |   |-- ui/
|   |   |   |-- css/
|   |   |   |-- js/
|   |   |   `-- index.html
|   |   |-- build.rs
|   |   |-- tauri.conf.json
|   |   `-- Cargo.toml
|   |-- opendl-engine/
|   |   |-- src/
|   |   |   |-- aria2_client.rs
|   |   |   |-- aria2_process.rs
|   |   |   |-- checkpoint.rs
|   |   |   |-- downloader.rs
|   |   |   |-- lib.rs
|   |   |   |-- probe.rs
|   |   |   |-- resolver.rs
|   |   |   |-- segment_allocator.rs
|   |   |   |-- storage.rs
|   |   |   |-- types.rs
|   |   |   `-- worker.rs
|   |   |-- tests/
|   |   |   |-- engine_tests.rs
|   |   |   `-- live_download_tests.rs
|   |   `-- Cargo.toml
|   `-- opendl-native-host/
|       |-- src/
|       |   |-- framing.rs
|       |   |-- install.rs
|       |   |-- main.rs
|       |   `-- parser.rs
|       |-- tests/
|       |   `-- native_host_tests.rs
|       `-- Cargo.toml
|-- extension/
|   |-- background.js
|   |-- content.js
|   |-- manifest.json
|   |-- popup.html
|   `-- popup.js
|-- specs/
|   |-- 01_download_engine_spec.md
|   |-- 02_native_messaging_spec.md
|   `-- 03_desktop_gui_spec.md
|-- .gitignore
|-- Cargo.lock
|-- Cargo.toml
`-- README.md
```

---

## Getting Started

### Prerequisites
- Windows 10 or Windows 11 (64-bit)
- Rust toolchain (stable channel, 1.78 or newer)
- Microsoft Edge Webview2 Runtime

### Building the Project
Clone the repository and build all workspace components in release mode:

```powershell
cargo build --release --workspace
```

The compiled binaries will be placed in `target\release\`:
- `opendl-desktop.exe`: GUI desktop application
- `opendl-cli.exe`: Command-line downloader and MCP tool
- `opendl-native-host.exe`: Native messaging host for browser integration

### Running the Desktop Application
Launch the graphical interface:

```powershell
.\target\release\opendl-desktop.exe
```

### Browser Extension Setup
1. Register the native messaging host in the Windows Registry:
   ```powershell
   .\target\release\opendl-native-host.exe --install
   ```
   This registers the host manifest for Google Chrome, Microsoft Edge, Brave, Vivaldi, and Mozilla Firefox.

2. Install the extension in your browser:
   - Open your Chromium-based browser and navigate to `chrome://extensions`.
   - Enable **Developer mode** using the toggle switch in the upper right.
   - Click **Load unpacked** and select the `extension` folder inside this repository.

### Command-Line Usage
Download a file directly from the terminal with 8 parallel connections:

```powershell
.\target\release\opendl-cli.exe https://example.com/archive.zip -c 8
```

---

## Verification and Testing

Run the automated test suite across all workspace crates:

```powershell
cargo test --workspace
```

The test suite covers:
- Core database CRUD, queue scheduling, and Named Pipe IPC roundtrips (`core_tests.rs`).
- Dynamic segment splitting, filename extraction, and storage integrity (`engine_tests.rs`).
- Direct stream resolution for Mediafire, Google Drive, and Dropbox (`resolver.rs`).
- Pure-Rust bencode parsing and SHA-1 info-hash calculation (`torrent.rs`).
- Length-prefixed stdio framing and native messaging payload parsing (`native_host_tests.rs`).
- Live HTTP download execution and checkpoint persistence (`live_download_tests.rs`).

---

## License

This project is licensed under the MIT License. See the LICENSE file for details.
