# rf-watch Android client

Native Kotlin + Jetpack Compose client for an rf-watch node: finds nodes on the LAN, shows
the node's own UI, raises drone alerts in the background and on Android Auto.

```
NsdDiscovery ──pick──▶ MonitorService (foreground, connectedDevice)
 (_rfwatch._tcp)          └─ NodeClient: OkHttp WebSocket /ws, reconnect with backoff
                                 │ ClientEvent
                                 ▼
                          NodeRepository.state  (StateFlow<MonitorState>)
                           │            │                 │
              NodeScreen status bar   ongoing +       DetectionsScreen
              + WebView (node UI)     alert notif.    (Android Auto)
```

## Features

- **Discovery**: Android NSD browses `_rfwatch._tcp`; each instance is resolved to
  host:port (`registerServiceInfoCallback` on API 34+, serial `resolveService` below).
  TXT `version` and `ws` (WebSocket path) are used. Manual `host:port` entry as fallback;
  port defaults to 8787.
- **Node screen**: WebView with the node's embedded Svelte UI (`http://host:port/`) under a
  native status bar: connection state and `DRONE DETECTED` / `clear` / `no data`, derived
  from the `/ws` stream, not from the WebView. A disconnected node shows "no data", never
  "clear".
- **Background monitoring**: selecting a node starts `MonitorService`, a foreground service
  of type `connectedDevice` with a persistent notification (status + Stop action). It owns
  the WebSocket (10 s pings, exponential backoff 0.5 s → 15 s, reset after data arrives),
  holds a partial wake lock while monitoring and keeps running when the app is backgrounded
  or the screen is off. Back on the node screen leaves monitoring running; **Stop** (in the
  app or the notification) ends it.
- **Drone alerts**: a high-priority notification for every new active `fhss_control` track
  (once per track id; a reconnect does not repeat it). `POST_NOTIFICATIONS` is requested on
  API 33+ when a node is opened. The alert carries a `CarAppExtender`, so it also appears
  as a heads-up in Android Auto.
- **Android Auto**: `RfWatchCarAppService` (Car App Library, category IOT) shows a
  `ListTemplate`: a **Status** row (`DRONE DETECTED` in red / `Clear` in green / `No data`,
  node and connection) and four **Emitter** rows with kind, centre MHz, peak dBm and
  confidence of the active tracks (drone links first, overflow summarised as "+N more").
  It observes the same `NodeRepository` as the phone UI.
- **Cleartext LAN HTTP**: `res/xml/network_security_config.xml` permits cleartext. Nodes
  are plain-HTTP services on arbitrary LAN IPs / `.local` names, which a domain-specific
  config cannot express.

### Car template refresh limits

The car host allows at most 5 templates per task. An update counts as a *refresh*, which
does not use up that quota, only if the template title, the number of rows and every row
title stay the same. So the car screen uses a fixed layout: constant title `rf-watch`, a
`Status` row and `Emitter 1..4` rows. All live values go into the row texts. Updates are
throttled to at most one per second, and only sent when the content changed.
`CarSummaryTest` checks that the layout has the same shape in every state.

## Build

Needs JDK 21 and the Android SDK with platform `android-37.2` (compileSdk 37, targetSdk 36,
minSdk 26).

```bash
echo "sdk.dir=$ANDROID_HOME" > local.properties
./gradlew test assembleDebug        # unit tests + debug APK
./gradlew lintDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

APK: `app/build/outputs/apk/debug/app-debug.apk`.

## Try it

```bash
# in the repo root
cargo run --release            # node on :8787, advertised as _rfwatch._tcp
```

- **Emulator**: NSD can't see the host's mDNS through the emulator's NAT. Use the manual
  entry `10.0.2.2:8787`.
- **Phone on the same Wi-Fi**: the node appears in the list. Tap it.

The `demo` scenario brings a drone in at 8 s of every 60 s loop, so an alert follows within
seconds. Press Home or turn the screen off and the alert still arrives.

## Android Auto with the Desktop Head Unit (DHU)

1. Install the DHU: `sdkmanager "extras;google;auto"`.
2. On the phone (or the Play-enabled emulator), install Android Auto, open its settings,
   tap **Version** ten times to unlock developer mode, then in the overflow menu choose
   **Developer settings** → enable **Unknown sources** (needed for apps not installed
   from Play) and **Start head unit server**.
3. Forward the port and start the DHU:
   ```bash
   adb forward tcp:5277 tcp:5277
   $ANDROID_HOME/extras/google/auto/desktop-head-unit
   ```
4. Start monitoring a node in the phone app, then open **rf-watch** from the DHU launcher.
   Debug builds accept any car host (`HostValidator.ALLOW_ALL_HOSTS_VALIDATOR`). Release
   builds only accept the Google hosts listed in `res/values/car_hosts.xml`.

Tip: Android Auto developer settings → **Enable debug overlay** shows the template step
counter. It should stay put while detections update.

## Tests

JVM unit tests (`app/src/test`):

| Test | Covers |
|---|---|
| `ProtocolTest` | `hello` / `frame` parsing as sent by the Rust server, unknown fields and kinds, malformed messages, `drone_detected` rule. |
| `MonitorStateTest` | Connection state machine, stale data cleared on disconnect, track ordering, one alert per new drone track, backoff. |
| `NodeAddressTest` | Manual `host:port` parsing (default port, scheme, IPv6). |
| `CarSummaryTest` | Car layout stays refresh-compatible; status and emitter rows. |
| `NodeClientTest` | WebSocket client end to end against MockWebServer, including reconnect. |

## Layout

| Path | Role |
|---|---|
| `model/Protocol.kt` | `/ws` messages (kotlinx.serialization), `droneDetected`. |
| `model/NodeAddress.kt` | Node address, URLs, manual input parsing. |
| `discovery/NsdDiscovery.kt` | NSD browse + resolve of `_rfwatch._tcp`. |
| `monitor/NodeClient.kt` | OkHttp WebSocket with reconnect (pure JVM). |
| `monitor/MonitorState.kt` | State, reducer, alert tracker, backoff. |
| `monitor/NodeRepository.kt` | Shared `StateFlow` for UI, notification and car. |
| `monitor/MonitorService.kt` | Foreground service. |
| `monitor/Notifications.kt` | Channels, ongoing and alert notifications. |
| `ui/` | Compose screens: discovery, node (status bar + WebView). |
| `car/` | Car App Library service, session, screen, `CarSummary`. |
