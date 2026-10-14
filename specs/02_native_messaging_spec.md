# Specification 02: Native Messaging Host & Browser Extension (`opendl-native-host`)

## 1. Scope & Objective
This specification defines the communication architecture for the OpenDL browser integration subsystem:
1. The Native Messaging Host executable (`opendl-native-host.exe`).
2. Protocol parsing:
   - **Modern JSON Protocol**: Length-prefixed standard JSON messaging used by the OpenDL Manifest V3 WebExtension and native clients.
   - Non-JSON / proprietary wire frames (`MSG#...`) are explicitly rejected/ignored.
3. IPC relay over Windows Named Pipe `\\.\pipe\opendl_core`.
4. Browser Registry Registration (`--install` / `--uninstall`).

---

## 2. Browser Native Messaging Framing

In Chromium (Chrome, Edge, Brave, Opera, Vivaldi) and Firefox, native messaging over stdio utilizes a 32-bit unsigned little-endian length prefix:

```
+-----------------------------------+-----------------------------------+
| 4 bytes: length N (uint32 LE)     | N bytes: UTF-8 Payload string     |
+-----------------------------------+-----------------------------------+
```

Maximum message size is $1\,\text{MB}$ in Chromium and Firefox.

---

## 3. Protocol Ingestion

The Native Messaging Host inspects incoming stdio payloads:

### 3.1 Clean JSON Protocol (OpenDL WebExtension)
```json
{
  "action": "download",
  "url": "https://example.com/file.zip",
  "filename": "file.zip",
  "referrer": "https://example.com/",
  "cookies": "session=abc123",
  "user_agent": "Mozilla/5.0 ...",
  "headers": {
    "Authorization": "Bearer ...",
    "Custom-Header": "value"
  }
}
```

Response:
```json
{
  "status": "ok",
  "task_id": "a5e8c1b2-...",
  "message": "Download task queued in opendl_core"
}
```

### 3.2 Non-JSON / Legacy Framing
Any non-JSON messages or legacy delimited format strings (e.g., `MSG#...`) are strictly ignored with `{"status": "ignored"}` and will not trigger downloads or handshakes.

---

## 4. Named Pipe IPC Relay (`\\.\pipe\opendl_core`)

When `opendl-native-host.exe` parses a download task:
1. It attempts to connect to the Windows Named Pipe `\\.\pipe\opendl_core`.
2. **Auto-Launch Fallback**:
   If the pipe is not active (i.e. GUI/daemon not running):
   The host locates `opendl-desktop.exe` in the application directory, spawns it in the background, and retries the pipe connection for up to 3 seconds.
3. Transmits the normalized download trigger:
   ```json
   {
     "action": "add_download",
     "url": "https://example.com/file.zip",
     "filename": "file.zip",
     "referrer": "https://example.com/",
     "cookies": "...",
     "user_agent": "..."
   }
   ```
4. Awaits the response from `opendl_core` and forwards acknowledgement back to the browser.

---

## 5. System Registration (`--install`)

The host binary supports automated registry installation:
- **Chrome / Chromium**:
  `HKCU\Software\Google\Chrome\NativeMessagingHosts\org.opendl.native`
  Points to manifest JSON with `"allowed_origins"`.
- **Microsoft Edge**:
  `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\org.opendl.native`
- **Mozilla Firefox**:
  `HKCU\Software\Mozilla\NativeMessagingHosts\org.opendl.native`
  Points to manifest JSON with `"allowed_extensions": ["opendl-extension@opendl.org"]`.
