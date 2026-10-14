# Specification 03: Desktop GUI Application (`opendl-desktop`)

## 1. Scope & Objective
This specification establishes the user interface, interaction model, state synchronization, and Tauri v2 integration for `opendl-desktop`, the graphical download management center for OpenDL.

---

## 2. User Interface Hierarchy & Layout

```
+---------------------------------------------------------------------------------------------------+
|  [+] Add URL  |  [>] Resume  |  [||] Pause  |  [X] Stop All  |  [-] Delete  |  [⚙] Options  | [🔍 Search] |
+-----------------------+---------------------------------------------------------------------------+
| CATEGORIES            | TASK LIST                                                                 |
| --------------------- | ------------------------------------------------------------------------- |
| [📁] All Downloads    | Name           Size       Progress        Speed      Time Left  Status     |
| [⏳] Unfinished (2)   | Ubuntu.iso     4.2 GB     [=========>--]  14.2 MB/s  3m 12s     Downloading|
| [✅] Finished (18)    | Archive.zip    150 MB     [============]  -          -          Complete   |
| --------------------- |                                                                           |
| [📦] Compressed       |                                                                           |
| [📄] Documents        |                                                                           |
| [🎵] Music            |                                                                           |
| [💻] Programs         |                                                                           |
| [🎬] Video            |                                                                           |
+-----------------------+---------------------------------------------------------------------------+
| Total Speed: 14.2 MB/s | Active Connections: 16 | Storage: D:\Downloads                          |
+---------------------------------------------------------------------------------------------------+
```

---

## 3. Dynamic Segment Visualizer

When clicking an active download, a dedicated detailed modal displays the **Segment Visualization Bar**:
- Total bar represents $0\%$ to $100\%$ of file length.
- Distinct color blocks represent:
  - **Dark Blue**: Inactive / unallocated byte ranges.
  - **Vibrant Green**: Downloaded & verified byte ranges stored on disk.
  - **Pulsing Cyan**: Active byte ranges currently streaming across open TCP connections.
- Each connection's thread ID, byte offset, current speed, and status are listed in real-time.

---

## 4. Tauri Commands & IPC Interface

The frontend communicates with the Rust backend via Tauri v2 commands:
- `add_task(url: String, filename: Option<String>, start_now: bool, connections: Option<usize>) -> TaskRecord`
- `list_tasks() -> Vec<TaskRecord>`
- `pause_task(id: String) -> bool`
- `resume_task(id: String) -> bool`
- `delete_task(id: String, delete_file: bool) -> bool`
- `get_all_progress() -> HashMap<String, DownloadProgress>`

---

## 5. Background IPC Daemon Integration

Upon startup, `opendl-desktop` launches the Windows Named Pipe server `\\.\pipe\opendl_core` on a background Tokio task.
When `opendl-native-host` forwards a download intercepted from Chrome or Firefox:
1. `opendl-desktop` receives the `add_download` IPC message.
2. Adds the task to the SQLite database and begins dynamic segmented download.
3. Updates the task list and system tray in real time.
