# Specification 01: Dynamic Segmented Download Engine (`opendl-engine`)

## 1. Scope & Objective
This specification describes the architecture, protocol negotiation, dynamic chunk splitting algorithms, and concurrent I/O model for `opendl-engine`, a clean-room Rust download accelerator for OpenDL.

---

## 2. Protocol & Header Negotiation

### 2.1 Metadata Probe (HEAD / Range-Probe GET)
Before establishing multi-connection download workers, the engine initiates a probe request to ascertain:
1. Total content length (`Content-Length`).
2. Server capability for byte ranges (`Accept-Ranges: bytes`).
3. Cache validation tokens (`ETag`, `Last-Modified`) to verify content consistency across connections.
4. Suggested filename (`Content-Disposition: attachment; filename="..."` or URL basename).

```http
HEAD /files/archive.iso HTTP/1.1
Host: downloads.example.com
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36
Accept: */*
Referer: https://example.com/
Connection: keep-alive
```

If the server returns `405 Method Not Allowed` or omits `Content-Length` on `HEAD`, the engine falls back to a ranged probe:
```http
GET /files/archive.iso HTTP/1.1
Host: downloads.example.com
Range: bytes=0-0
```

### 2.2 Range Support Assessment
- **Range Supported**: HTTP `206 Partial Content` returned with `Content-Range: bytes 0-0/104857600`. The engine engages **Dynamic Multi-Connection Segmentation**.
- **Range Not Supported**: HTTP `200 OK` or `Accept-Ranges: none`. The engine falls back to **Single-Stream Streaming Download**.

---

## 3. Dynamic Segmentation Algorithm

Unlike static chunking (which partitions a file into $N$ immutable slices $[0, S/N), [S/N, 2S/N), \dots$), the Dynamic Segmentation Algorithm continually rebalances work across all available connections.

### 3.1 Parameters
- $C_{\max}$: Maximum concurrent connections (configurable, default: 8, max: 32).
- $T_{\min}$: Minimum split threshold (e.g., $1\,\text{MB} = 1,048,576\,\text{bytes}$). A segment will not be split if its remaining uncompleted bytes $< 2 \times T_{\min}$.
- $S$: Total file size in bytes.

### 3.2 Lifecycle & Rebalancing
1. **Initial Allocation**:
   - The file is initially divided into $K = \min(C_{\max}, \lfloor S / T_{\min} \rfloor)$ primary ranges.
   - $K$ concurrent workers are spawned, each requesting `Range: bytes={start}-{end}`.
2. **Streaming & Progress Accounting**:
   - Each worker $i$ maintains:
     - `start`: Initial byte offset assigned.
     - `current`: Offset up to which data has been durably written to disk.
     - `end`: Upper bound byte offset assigned (inclusive).
     - `remaining`: $\max(0, \text{end} - \text{current})$.
3. **Dynamic Split on Worker Completion**:
   - When any worker $W_{\text{idle}}$ reaches its `end`, it notifies the Coordinator.
   - The Coordinator inspects all currently active workers $\{W_1, \dots, W_a\}$.
   - It identifies the active worker $W_{\text{largest}}$ with the maximum remaining bytes:
     $$\text{rem}_{\max} = \max_j (W_j.\text{end} - W_j.\text{current})$$
   - If $\text{rem}_{\max} \ge 2 \times T_{\min}$:
     - Let $W_{\text{split}} = W_{\text{largest}}$.
     - Calculate midpoint:
       $$\text{split\_point} = W_{\text{split}}.\text{current} + \left\lfloor \frac{W_{\text{split}}.\text{end} - W_{\text{split}}.\text{current}}{2} \right\rfloor$$
     - Atomically truncate $W_{\text{split}}$'s boundary:
       $$W_{\text{split}}.\text{end} \leftarrow \text{split\_point}$$
     - Assign the new sub-range to $W_{\text{idle}}$:
       $$W_{\text{idle}}.\text{start} \leftarrow \text{split\_point} + 1,\quad W_{\text{idle}}.\text{current} \leftarrow \text{split\_point} + 1,\quad W_{\text{idle}}.\text{end} \leftarrow \text{original\_end}$$
     - $W_{\text{idle}}$ sends a new HTTP request:
       $$\text{Range: bytes}=(\text{split\_point} + 1)-\text{original\_end}$$
4. **Completion**:
   - When all segments have `current > end`, the file is 100% complete.

---

## 4. Concurrent Direct-Offset Disk I/O

To eliminate post-download file stitching (concatenation) latency:
1. **Preallocation**:
   - When total size $S$ is known, the destination file is immediately preallocated to length $S$ using `tokio::fs::File::set_len(S)`.
2. **Positioned Writes**:
   - Workers write received byte buffers directly into their respective offsets:
     - On Windows: using positional `seek` + `write_all` on independent cloned handles, or `std::os::windows::fs::FileExt::seek_write`.
   - Because byte ranges are non-overlapping partitions, workers write concurrently without lock contention or buffer collisions.

---

## 5. Checkpoints & Resumption Schema

Download state is recorded in a checkpoint state:
```json
{
  "url": "https://example.com/file.zip",
  "destination": "D:\\Downloads\\file.zip",
  "total_size": 104857600,
  "etag": "\"3f8a-5928b1\"",
  "last_modified": "Mon, 12 Oct 2026 00:00:00 GMT",
  "segments": [
    { "id": 0, "start": 0, "current": 25000000, "end": 26214399, "status": "active" },
    { "id": 1, "start": 26214400, "current": 52428800, "end": 52428800, "status": "completed" }
  ]
}
```

Upon resuming a paused or interrupted download:
1. The engine checks if the server file changed via `If-Range` or `If-Match` against the stored `ETag` / `Last-Modified`.
2. For all incomplete segments where `current <= end`, new requests are launched with `Range: bytes={current}-{end}`.
3. Completed segments (`current > end`) are skipped.

---

## 6. Rate Limiting (Token Bucket)

When the user specifies a maximum transfer speed $R$ (bytes/sec):
- A shared token bucket with capacity $R$ is refilled every $\Delta t$ (e.g., 50ms) with $R \times \Delta t$ tokens.
- All active workers acquire tokens prior to reading from their HTTP response streams.
