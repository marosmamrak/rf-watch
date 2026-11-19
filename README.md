# rf-watch

A small **passive RF drone-link detector** demo: a Rust service that sweeps the 2.4 GHz
ISM band, detects the frequency-hopping remote-control link of a drone among Wi-Fi and
other background traffic, and streams spectrum, waterfall and detections over WebSocket
to an embedded **Svelte** UI. The node advertises itself on the LAN over **mDNS**, so a
mobile client can find it without an IP address.

The RF front end is **simulated** (no SDR needed); the detector only sees power spectra,
the same input it would get from an `rtl_power` / HackRF sweep.

```
 Simulator ──sweeps──▶ Detector ──report──▶ broadcast ──▶ /ws  (JSON frames, 10 Hz)
 (sim.rs)             (detect.rs)          (server.rs)    /api/status
                                                          /     (embedded Svelte UI)
 mDNS: _rfwatch._tcp.local.  (mdns.rs)
```

## Detection

1. **Noise floor** = 25th percentile of the sweep (robust while part of the band is busy);
   threshold = floor + 10 dB.
2. **Segments**: contiguous runs of bins above the threshold.
3. **Wideband** segments (≥ 8 MHz) are tracked by centre frequency with an exponential
   hit rate — Wi-Fi, video downlinks.
4. **Narrowband** bursts (≤ 2.5 MHz) are pooled over a 2 s window. ≥ 2.5 bursts per sweep
   spread over ≥ 15 distinct 1 MHz channels is reported as an **FHSS control link**;
   isolated blips (Bluetooth, clicks) are not. Lost tracks stay listed for 10 s.

The `demo` scenario loops every 60 s: background traffic, a drone flies in at 8 s (signal
rises), passes and leaves at 40 s.

## Run

Needs Rust ≥ 1.85 and Node ≥ 20.

```bash
(cd web && npm ci && npm run build)   # UI is embedded into the binary at compile time
cargo run --release
# open http://localhost:8787
```

Options: `--scenario demo|quiet|drone`, `--fps 10`, `--name rf-watch`, `--listen 0.0.0.0:8787`,
`--seed N`, `--no-mdns`. Logs via `RUST_LOG=debug`.

UI development with hot reload (proxies `/ws` and `/api` to the Rust server on :8787):

```bash
cargo run            # terminal 1
cd web && npm run dev   # terminal 2 → http://localhost:5173
```

The UI connects to `/ws` on the host that served it; `?ws=ws://host:port/ws` overrides
it (e.g. in a WebView that loads bundled assets).

Find the node on the LAN:

```bash
dns-sd -B _rfwatch._tcp        # macOS
avahi-browse -r _rfwatch._tcp  # Linux
```

## Protocol

`GET /ws` — first message `hello`, then one `frame` per sweep:

```json
{"type":"hello","device":"rf-watch","version":"0.1.0",
 "band":{"start_mhz":2400.0,"stop_mhz":2483.5,"bins":256,"fps":10}}

{"type":"frame","seq":812,"t_ms":81200,"noise_floor_dbm":-96.8,"threshold_dbm":-86.8,
 "power_dbm":[-95.9,-97.1,...],
 "tracks":[{"id":3,"kind":"fhss_control","center_mhz":2441.8,"bandwidth_mhz":72.0,
            "peak_dbm":-61.2,"confidence":0.9,"first_seen_ms":10100,
            "last_seen_ms":81200,"active":true}]}
```

`GET /api/status` — snapshot: device, frames processed, connected clients,
`drone_detected`, current tracks.

## Tests

```bash
cargo test                  # simulator, detector (false alarms, detection latency,
                            # track lifecycle), HTTP, WebSocket end to end
cd web && npm test && npm run check
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy, tests and both builds.

## Layout

| Path | Role |
|---|---|
| `src/sim.rs` | Simulated sweeps: noise, bursty Wi-Fi, blips, hopping control link. |
| `src/detect.rs` | Noise floor, segmentation, wideband tracking, FHSS detection. |
| `src/server.rs` | Scanner loop, broadcast, axum routes, embedded UI. |
| `src/mdns.rs` | `_rfwatch._tcp` advertisement. |
| `src/protocol.rs` | JSON messages. |
| `web/src/lib/` | Connection with reconnect, spectrum, waterfall, emitter list. |

## Ideas

- Real input: parse `rtl_power` / `hackrf_sweep` CSV as a second sweep source.
- 5.8 GHz band and video-downlink classification.
- Android client: discover `_rfwatch._tcp` via NSD, stream `/ws`, show alerts and the
  UI in a WebView, Android Auto notification.
