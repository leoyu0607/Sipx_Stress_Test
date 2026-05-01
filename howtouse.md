# sipress — How to Use

## Table of Contents
1. [Download & Install](#1-download--install)
2. [GUI Quickstart](#2-gui-quickstart)
3. [Configure Your Test](#3-configure-your-test)
4. [Run a Test & Read Live Metrics](#4-run-a-test--read-live-metrics)
5. [View Results](#5-view-results)
6. [CLI Usage](#6-cli-usage)
7. [Troubleshooting](#7-troubleshooting)

---

## 1. Download & Install

### Choose a build for your platform

| Platform | Installer (recommended) | Portable (no install) |
|---|---|---|
| Windows x64 | `sipress-gui-windows-x86_64-installer.msi` or `*-setup.exe` | `sipress-gui-windows-x86_64-portable.exe` |
| Linux x64 | `sipress-gui-linux-x86_64-installer.deb` | `sipress-gui-linux-x86_64-portable.AppImage` |
| macOS x64 | `sipress-gui-macos-x86_64-installer.dmg` | `sipress-gui-macos-x86_64-portable` |
| macOS ARM | `sipress-gui-macos-arm64-installer.dmg` | `sipress-gui-macos-arm64-portable` |

### Installer vs Portable

- **Installer** registers the app with your OS (Start Menu shortcut, uninstaller, etc.). Use this for day-to-day desktop use.
- **Portable** is a single self-contained file. Copy it anywhere, run it directly — nothing is written to your system. Good for servers or shared machines.

### Windows

1. Download the `.msi` or `-setup.exe` installer and run it.
2. Follow the wizard — accept the license, choose destination, click Install.
3. Launch **sipress** from the Start Menu or desktop shortcut.

> **Portable:** Download `*-portable.exe`, double-click to run. No installation needed.

### Linux

```bash
# Debian/Ubuntu — installer
sudo dpkg -i sipress-gui-linux-x86_64-installer.deb

# AppImage — portable (no install)
chmod +x sipress-gui-linux-x86_64-portable.AppImage
./sipress-gui-linux-x86_64-portable.AppImage
```

### macOS

1. Open the `.dmg` file and drag **sipress** to your Applications folder.
2. Launch from Applications or Spotlight.

> **First launch:** macOS may block unsigned apps. Go to **System Settings → Privacy & Security** and click **Open Anyway**.

---

## 2. GUI Quickstart

When sipress opens you'll see:

```
┌─ Title bar ──────────────────────────────── [─] [□] [✕] ─┐
│  ┌─ Sidebar ──────────┐  ┌─ Main panel ──────────────────┐ │
│  │  Test Config       │  │  Metrics / Live charts        │ │
│  │  • Server          │  │                               │ │
│  │  • Transport       │  │  Logs                         │ │
│  │  • Caller settings │  │                               │ │
│  │  • Duration        │  │                               │ │
│  │                    │  │                               │ │
│  │  [▶ Start]         │  │                               │ │
│  └────────────────────┘  └───────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
```

- **Sidebar** — fill in your target server and call parameters, then press **Start**.
- **Main panel** — shows live CPS, concurrent calls, ASR, PDD and a scrolling log.
- **Title bar** — draggable; close/minimize/maximize buttons on the right.

---

## 3. Configure Your Test

All settings are in the left sidebar.

### Server

| Field | Example | Notes |
|---|---|---|
| Server address | `192.168.1.100:5060` | SIP proxy IP and port |
| Transport | `UDP` / `TCP` / `TLS` | Match your server config |
| Local port | `5080` | Leave blank for OS-assigned |

### Caller

| Field | Example | Notes |
|---|---|---|
| Caller number | `+886912345678` | From-header number |
| CPS | `10` | Calls per second |
| Concurrency | `50` | Max simultaneous calls |

### Test Duration

Set how many seconds the test should run. The countdown timer appears once started.

### Audio (optional)

Enable the **Audio** toggle to send RTP audio during calls.  
Click **Browse** to select a `.wav`, `.pcm`, or `.raw` file.  
Leave disabled for signalling-only tests (faster, less CPU).

---

## 4. Run a Test & Read Live Metrics

1. Fill in all required fields (server address and caller number at minimum).
2. Click **▶ Start**.
3. Watch the metrics panel update every second:

| Metric | What it means |
|---|---|
| **CPS** | Calls initiated per second (real-time delta) |
| **Concurrent** | Calls currently in progress |
| **Total initiated** | Cumulative calls sent since test start |
| **ASR** | Answer-Seizure Ratio — `answered / initiated × 100 %` |
| **PDD (ms)** | Post-Dial Delay — average time from INVITE to 180/200 |
| **MOS** | Mean Opinion Score for audio quality (0–5, needs RTP) |

4. The **Logs** panel shows real-time SIP events (INVITE, 200 OK, BYE, errors).

5. To stop early, click **■ Stop**. The test will finalize and show the summary report.

---

## 5. View Results

When the test finishes (timer expires or you click Stop) the report panel shows:

- Total calls initiated / answered / failed
- Final ASR %
- Average / min / max PDD
- Average MOS (if RTP was enabled)
- Per-error-code breakdown (e.g. 404, 486, 503 counts)

Results are also saved as a JSON file in the `logs/` directory next to the executable.

---

## 6. CLI Usage

The `sipress` CLI is for scripted or headless environments.

### Basic run

```bash
sipress run \
  --server 192.168.1.100:5060 \
  --caller +886912345678 \
  --cps 10 \
  --concurrency 50 \
  --duration 60
```

### All flags

```
USAGE:
    sipress run [OPTIONS]

OPTIONS:
    --server <ADDR>         SIP server address (host:port)
    --transport <T>         udp | tcp | tls  [default: udp]
    --local-port <PORT>     Local SIP port (optional)
    --caller <NUM>          Caller number (From header)
    --cps <N>               Calls per second  [default: 10]
    --concurrency <N>       Max concurrent calls  [default: 50]
    --duration <SECS>       Test duration in seconds  [default: 60]
    --call-duration <SECS>  Max call hold time  [default: 30]
    --invite-timeout <SECS> INVITE timeout  [default: 8]
    --audio <FILE>          WAV/PCM file for RTP audio
    --rtp-base-port <PORT>  First RTP port  [default: 10000]
    --logs-dir <DIR>        Log output directory  [default: logs]
```

### Example — 30-second TLS test with audio

```bash
sipress run \
  --server sip.example.com:5061 \
  --transport tls \
  --caller 1000 \
  --cps 5 \
  --concurrency 20 \
  --duration 30 \
  --audio samples/speech_8k.wav
```

---

## 7. Troubleshooting

### App won't launch on macOS

Go to **System Settings → Privacy & Security → Open Anyway** for sipress.  
Or run `xattr -c sipress-gui-macos-*-portable` in Terminal first.

### No calls are being sent

- Verify the server address and port are reachable (`ping`, `nc -u <host> <port>`).
- Check that your firewall allows outbound UDP/TCP on port 5060.
- Confirm the caller number format matches what the server expects.

### All calls return 403 / 401

Your SIP server requires authentication. sipress currently supports unauthenticated INVITE flows; configure your server to whitelist the test source IP or disable auth for the test trunk.

### High failed-call count

- **486 Busy Here** — Server is overloaded; reduce CPS or concurrency.
- **503 Service Unavailable** — Server is down or rejecting connections.
- **408 Timeout** — Network latency or server not responding; check connectivity.

### MOS shows 0 / N/A

MOS is only calculated when **Audio** is enabled and RTP packets are exchanged.  
Make sure you selected a valid `.wav` / `.pcm` file and the server is accepting RTP.

### GUI shows blank window

The frontend failed to load. Try:
1. Delete `%APPDATA%\com.leozh.sipress` (Windows) or `~/.config/com.leozh.sipress` (Linux).
2. Reinstall using the installer package.

### Log files location

Logs are written to the `logs/` directory in the same folder as the executable (portable) or in your user data directory (installed version).

---

## Need help?

Open an issue at the project repository or check the `README.md` for architecture details and build instructions.
