# OpenDL

A high-performance, open-source dynamic download accelerator written in pure Rust. Features dynamic segment rebalancing, native browser messaging integration, modern desktop GUI, and CLI.

---

## 🌟 Key Features

1. **Dynamic Segment Allocation Engine (`opendl-engine`)**:
   - Dynamic rebalancing algorithm: when any connection finishes its segment early, the coordinator dynamically splits the largest incomplete active segment into two and immediately assigns the second half to the idle worker.
   - Zero file-stitching / concatenation latency: uses preallocated direct-offset random-access writes (`seek_write`) on disk.
   - Comprehensive crash resilience and resume checkpoints.

2. **Browser Integration (`opendl-native-host`)**:
   - **Native Host API**: Modern, clean JSON framing over length-prefixed stdio (`org.opendl.native`).
   - **Manifest V3 WebExtension**: Bundled modern extension supporting download interception, context menus ("Download with OpenDL"), and cookie forwarding.
   - Communicates with the core application via Windows Named Pipes (`\\.\pipe\opendl_core`).
   - Automated registration CLI (`--install` / `--uninstall`).

3. **Desktop Application (`opendl-desktop`)**:
   - Built on Tauri v2 and Webview2 with sub-30MB RAM footprint.
   - Clean, dark-mode management interface with category filtering (Compressed, Documents, Music, Programs, Video).
   - **Dynamic Segment Visualization**: Real-time multi-color progress blocks displaying streaming TCP connection slices and downloaded ranges.
   - SQLite persistence with Write-Ahead Logging (WAL) mode.

4. **Command-Line Accelerator (`opendl-cli`)**:
   - Standalone CLI for scripted and terminal downloading with interactive Indicatif progress bars and connection stats.

---

## 📁 Repository Structure

```
opendl\
├── specs\                          # Technical specifications
│   ├── 01_download_engine_spec.md  # Dynamic chunk splitting math & I/O
│   ├── 02_native_messaging_spec.md # Native messaging schemas
│   └── 03_desktop_gui_spec.md      # UI layout, commands & segment visualizer
├── crates\
│   ├── opendl-engine\              # Tokio dynamic segmentation engine
│   ├── opendl-core\                # SQLite WAL DB & Windows Named Pipe IPC
│   ├── opendl-native-host\         # Browser stdio native messaging host
│   ├── opendl-cli\                 # Command-line downloader
│   └── opendl-desktop\             # Tauri v2 Desktop GUI
├── extension\                      # Manifest V3 browser extension
└── target\release\                 # Optimized release binaries
    ├── opendl-cli.exe
    ├── opendl-desktop.exe
    └── opendl-native-host.exe
```

---

## 🚀 Quick Start

### 1. Run the Desktop Application
```powershell
.\target\release\opendl-desktop.exe
```

### 2. Download via CLI
```powershell
.\target\release\opendl-cli.exe https://example.com/file.iso -c 8
```

### 3. Register Browser Integration
```powershell
.\target\release\opendl-native-host.exe --install
```
This registers `org.opendl.native` in the Windows Registry for Google Chrome, Microsoft Edge, and Mozilla Firefox.

### 4. Load Browser Extension
1. Open Chrome/Edge and navigate to `chrome://extensions`.
2. Enable **Developer mode**.
3. Click **Load unpacked** and select the `extension\` directory.

---

## 🧪 Testing

Run all unit and live HTTP integration tests across the workspace:
```powershell
cargo test --workspace
```
