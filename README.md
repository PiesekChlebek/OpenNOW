<h1 align="center">OpenNOW</h1>

<p align="center">
  <strong>Open source GeForce NOW client built from the ground up in Native Rust</strong>
</p>

<p align="center">
  <a href="https://github.com/zortos293/OpenNOW/releases">
    <img src="https://img.shields.io/github/v/tag/zortos293/OpenNOW?style=for-the-badge&label=Download&color=brightgreen" alt="Download">
  </a>
  <a href="https://opennow.zortos.me">
    <img src="https://img.shields.io/badge/Docs-opennow.zortos.me-blue?style=for-the-badge" alt="Documentation">
  </a>
  <a href="https://discord.gg/8EJYaJcNfD">
    <img src="https://img.shields.io/badge/Discord-Join%20Us-7289da?style=for-the-badge&logo=discord&logoColor=white" alt="Discord">
  </a>
</p>

<p align="center">
  <a href="https://github.com/zortos293/OpenNOW/stargazers">
    <img src="https://img.shields.io/github/stars/zortos293/OpenNOW?style=flat-square" alt="Stars">
  </a>
  <a href="https://github.com/zortos293/OpenNOW/releases">
    <img src="https://img.shields.io/github/downloads/zortos293/OpenNOW/total?style=flat-square" alt="Downloads">
  </a>
  <a href="https://github.com/zortos293/OpenNOW/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/zortos293/OpenNOW?style=flat-square" alt="License">
  </a>
</p>

---

> **Warning**  
> OpenNOW is under **active development**. Expect bugs and performance issues.  
> Check the [Known Issues](#known-issues) section and [full documentation](https://opennow.zortos.me) for details.

---

## About

OpenNOW is a custom GeForce NOW client rewritten entirely in **Native Rust** for maximum performance and lower resource usage. Built with `wgpu` and `iced` for a seamless cloud gaming experience.

<table>
<tr>
<td width="50%">

**Why OpenNOW?**
- Native Performance (Rust + wgpu)
- No artificial FPS/resolution/bitrate limits
- No telemetry by default
- Cross-platform (Windows, macOS, Linux)

</td>
<td width="50%">

**Key Features**
- Zero-copy hardware decoding
- Raw input capture (low latency)
- Gamepad & racing wheel support
- Alliance Partner support

</td>
</tr>
</table>

📖 **Full Documentation:** [opennow.zortos.me](https://opennow.zortos.me)

---

## Quick Start

### One-Line Installer (macOS & Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/zortos293/OpenNOW/main/scripts/install.sh | bash
```

This will automatically:
- Detect your OS and architecture
- Install GStreamer dependencies
- Download and install the latest release
- Create desktop shortcuts (Linux)

### Manual Download

| Platform | Download | Notes |
|----------|----------|-------|
| **Windows x64** | [OpenNOW-windows-x64.zip](https://github.com/zortos293/OpenNOW/releases/latest) | Portable, GStreamer bundled |
| **macOS** | [OpenNOW-macos-arm64.zip](https://github.com/zortos293/OpenNOW/releases/latest) | Universal (M1/M2/M3 & Intel) |
| **Linux x64** | [OpenNOW-linux-x64.AppImage](https://github.com/zortos293/OpenNOW/releases/latest) | AppImage, GStreamer bundled |
| **Linux ARM64** | [OpenNOW-linux-arm64.zip](https://github.com/zortos293/OpenNOW/releases/latest) | Requires system GStreamer |

> **macOS:** If blocked by Gatekeeper, run: `xattr -d com.apple.quarantine OpenNOW.app`

### GStreamer Requirements (Manual Install Only)

If installing manually (not using the one-liner), you need GStreamer:

| Platform | Installation |
|----------|--------------|
| **Windows x64** | Bundled with release (no action needed) |
| **macOS** | `brew install gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav` |
| **Linux (apt)** | `sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-vaapi` |
| **Linux (dnf)** | `sudo dnf install gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-ugly-free gstreamer1-libav gstreamer1-vaapi` |

---

## Platform Support

| Platform | Status | Hardware Decoding |
|----------|:------:|-------------------|
| Windows x64 | ✅ Working | GStreamer D3D11VA (NVIDIA, AMD, Intel) |
| Windows ARM64 | ❌ Not Supported | No GStreamer ARM64 binaries |
| macOS ARM64 | ✅ Working | GStreamer VideoToolbox |
| macOS Intel | ✅ Working | GStreamer VideoToolbox |
| Linux x64 | ⚠️ Buggy | GStreamer (VAAPI/V4L2) |
| Linux ARM64 | ⚠️ Buggy | GStreamer (V4L2) |
| Raspberry Pi | ❌ Broken | Under investigation |

---

## Features

| Feature | Status | Feature | Status |
|---------|:------:|---------|:------:|
| Authentication | ✅ | Gamepad Support | ✅ |
| Game Library | ✅ | Audio Playback | ✅ |
| WebRTC Streaming | ✅ | Stats Overlay | ✅ |
| Hardware Decoding | ✅ | Anti-AFK | ✅ |
| Zero-Copy Rendering | ✅ | Alliance Partners | ✅ |
| Mouse/Keyboard | ✅ | Clipboard Paste | ✅ |
| AV1 Codec | ✅ | H.264/H.265 | ✅ |

**Coming Soon:** Microphone, Instant Replay, Screenshots, Plugin System, Theming

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `F3` | Toggle stats overlay |
| `F8` | Toggle mouse capture |
| `F11` | Toggle fullscreen |
| `Ctrl+Shift+Q` | Quit session |
| `Ctrl+Shift+F10` | Toggle anti-AFK |

---

## Known Issues

| Issue | Workaround |
|-------|------------|
| High CPU usage | Lower FPS/resolution in settings |
| Green screen flashes | Switch to H.264 codec |
| Audio stuttering | Restart stream |
| Laggy input | Enable `low_latency_mode` |
| Linux instability | Use Windows/macOS for now |

---

## Building from Source

### Prerequisites

**All platforms require GStreamer development libraries:**

| Platform | Install Command |
|----------|-----------------|
| **Windows** | Download from [GStreamer.freedesktop.org](https://gstreamer.freedesktop.org/download/) (MSVC 64-bit) |
| **macOS** | `brew install gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav pkg-config` |
| **Linux (apt)** | `sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev` |
| **Linux (dnf)** | `sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel` |

### Build

```bash
git clone https://github.com/zortos293/OpenNOW.git
cd OpenNOW/opennow-streamer
cargo build --release
```

See the [full build guide](https://opennow.zortos.me/guides/getting-started/) for platform-specific requirements.

---

## Documentation

Full documentation available at **[opennow.zortos.me](https://opennow.zortos.me)**

- [Getting Started](https://opennow.zortos.me/guides/getting-started/)
- [Architecture Overview](https://opennow.zortos.me/architecture/overview/)
- [Configuration Reference](https://opennow.zortos.me/reference/configuration/)
- [WebRTC Protocol](https://opennow.zortos.me/reference/webrtc/)

---

## Support the Project

OpenNOW is a passion project developed in my free time. If you enjoy using it, please consider sponsoring!

<p align="center">
  <a href="https://github.com/sponsors/zortos293">
    <img src="https://img.shields.io/badge/Sponsor_on_GitHub-EA4AAA?style=for-the-badge&logo=github-sponsors&logoColor=white" alt="Sponsor">
  </a>
</p>

---

## Disclaimer

This is an **independent project** not affiliated with NVIDIA Corporation. Created for educational purposes. GeForce NOW is a trademark of NVIDIA. Use at your own risk.

---

<p align="center">
  <a href="https://opennow.zortos.me">Documentation</a> · 
  <a href="https://discord.gg/8EJYaJcNfD">Discord</a> · 
  <a href="https://github.com/sponsors/zortos293">Sponsor</a>
</p>

<p align="center">
  Made with ❤️ by <a href="https://github.com/zortos293">zortos293</a>
</p>
