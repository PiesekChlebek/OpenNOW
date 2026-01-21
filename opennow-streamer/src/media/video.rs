//! Video Decoder
//!
//! Hardware-accelerated H.264/H.265 decoding.
//!
//! Platform-specific backends:
//! - Windows: Native DXVA (D3D11 Video API)
//! - macOS: FFmpeg with VideoToolbox
//! - Linux: Native Vulkan Video or GStreamer
//!
//! This module provides both blocking and non-blocking decode modes:
//! - Blocking: `decode()` - waits for result (legacy, causes latency)
//! - Non-blocking: `decode_async()` - fire-and-forget, writes to SharedFrame

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::mpsc as tokio_mpsc;

#[cfg(target_os = "linux")]
use std::thread;

#[cfg(target_os = "windows")]
use std::path::Path;

use super::VideoFrame;
#[cfg(target_os = "linux")]
use super::{ColorRange, ColorSpace, PixelFormat, TransferFunction};
use crate::app::{config::VideoDecoderBackend, SharedFrame, VideoCodec};

// Note: FFmpeg has been replaced by GStreamer on macOS for better Intel compatibility.
// The following FFmpeg imports are kept only for potential future use or reference.
// macOS now uses GStreamer with VideoToolbox (vtdec) for hardware-accelerated decoding.

/// GPU Vendor for decoder optimization
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum GpuVendor {
    Nvidia,
    Intel,
    Amd,
    Apple,
    Broadcom, // Raspberry Pi VideoCore
    Other,
    Unknown,
}

/// Cached GPU vendor
static GPU_VENDOR: std::sync::OnceLock<GpuVendor> = std::sync::OnceLock::new();

/// Detect the primary GPU vendor using wgpu, prioritizing discrete GPUs
pub fn detect_gpu_vendor() -> GpuVendor {
    *GPU_VENDOR.get_or_init(|| {
        // blocked_on because we are in a sync context (VideoDecoder::new)
        // but wgpu adapter request is async
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default()); // Needs borrow

            // Enumerate all available adapters (wgpu 28 returns a Future)
            let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;

            let mut best_score = -1;
            let mut best_vendor = GpuVendor::Unknown;

            info!("Available GPU adapters:");

            for adapter in adapters {
                let info = adapter.get_info();
                let name = info.name.to_lowercase();
                let mut score = 0;
                let mut vendor = GpuVendor::Other;

                // Identify vendor
                if name.contains("nvidia") || name.contains("geforce") || name.contains("quadro") {
                    vendor = GpuVendor::Nvidia;
                    score += 100;
                } else if name.contains("amd") || name.contains("adeon") || name.contains("ryzen") {
                    vendor = GpuVendor::Amd;
                    score += 80;
                } else if name.contains("intel")
                    || name.contains("uhd")
                    || name.contains("iris")
                    || name.contains("arc")
                {
                    vendor = GpuVendor::Intel;
                    score += 50;
                } else if name.contains("apple")
                    || name.contains("m1")
                    || name.contains("m2")
                    || name.contains("m3")
                {
                    vendor = GpuVendor::Apple;
                    score += 90; // Apple Silicon is high perf
                } else if name.contains("videocore")
                    || name.contains("broadcom")
                    || name.contains("v3d")
                    || name.contains("vc4")
                {
                    vendor = GpuVendor::Broadcom;
                    score += 30; // Raspberry Pi - low power device
                }

                // Prioritize discrete GPUs
                match info.device_type {
                    wgpu::DeviceType::DiscreteGpu => {
                        score += 50;
                    }
                    wgpu::DeviceType::IntegratedGpu => {
                        score += 10;
                    }
                    _ => {}
                }

                info!(
                    "  - {} ({:?}, Vendor: {:?}, Score: {})",
                    info.name, info.device_type, vendor, score
                );

                if score > best_score {
                    best_score = score;
                    best_vendor = vendor;
                }
            }

            if best_vendor != GpuVendor::Unknown {
                info!("Selected best GPU vendor: {:?}", best_vendor);
                best_vendor
            } else {
                // Fallback to default request if enumeration fails
                warn!("Adapter enumeration yielded no results, trying default request");

                let adapter_result = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface: None,
                        force_fallback_adapter: false,
                    })
                    .await;

                // Handle Result
                if let Ok(adapter) = adapter_result {
                    let info = adapter.get_info();
                    let name = info.name.to_lowercase();

                    if name.contains("nvidia") {
                        GpuVendor::Nvidia
                    } else if name.contains("intel") {
                        GpuVendor::Intel
                    } else if name.contains("amd") {
                        GpuVendor::Amd
                    } else if name.contains("apple") {
                        GpuVendor::Apple
                    } else if name.contains("videocore")
                        || name.contains("broadcom")
                        || name.contains("v3d")
                    {
                        GpuVendor::Broadcom
                    } else {
                        GpuVendor::Other
                    }
                } else {
                    GpuVendor::Unknown
                }
            }
        })
    })
}

/// Check if Intel QSV runtime is available on the system
/// Returns true if the required DLLs are found
#[cfg(target_os = "windows")]
fn is_qsv_runtime_available() -> bool {
    use std::env;

    // Intel Media SDK / oneVPL runtime DLLs to look for
    let runtime_dlls = [
        "libmfx-gen.dll", // Intel oneVPL runtime (11th gen+, newer)
        "libmfxhw64.dll", // Intel Media SDK runtime (older)
        "mfxhw64.dll",    // Alternative naming
        "libmfx64.dll",   // Another variant
    ];

    // Check common paths where Intel runtimes are installed
    let search_paths: Vec<std::path::PathBuf> = vec![
        // System32 (most common for driver-installed runtimes)
        env::var("SystemRoot")
            .map(|s| Path::new(&s).join("System32"))
            .unwrap_or_default(),
        // SysWOW64 for 32-bit
        env::var("SystemRoot")
            .map(|s| Path::new(&s).join("SysWOW64"))
            .unwrap_or_default(),
        // Intel Media SDK default install
        Path::new(
            "C:\\Program Files\\Intel\\Media SDK 2023 R1\\Software Development Kit\\bin\\x64",
        )
        .to_path_buf(),
        Path::new("C:\\Program Files\\Intel\\Media SDK\\bin\\x64").to_path_buf(),
        // oneVPL default install
        Path::new("C:\\Program Files (x86)\\Intel\\oneAPI\\vpl\\latest\\bin").to_path_buf(),
        // Application directory (for bundled DLLs)
        env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default(),
    ];

    for dll in &runtime_dlls {
        for path in &search_paths {
            let full_path = path.join(dll);
            if full_path.exists() {
                info!("Found Intel QSV runtime: {}", full_path.display());
                return true;
            }
        }
    }

    // Also try loading via Windows DLL search path
    // If Intel drivers are installed, the DLLs should be in PATH
    if let Ok(output) = std::process::Command::new("where")
        .arg("libmfx-gen.dll")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            info!("Found Intel QSV runtime via PATH: {}", path.trim());
            return true;
        }
    }

    debug!("Intel QSV runtime not found - QSV decoder will be skipped");
    false
}

#[cfg(not(target_os = "windows"))]
fn is_qsv_runtime_available() -> bool {
    // On Linux, check for libmfx.so or libvpl.so
    use std::process::Command;

    // QSV is only supported on Intel architectures
    if !cfg!(target_arch = "x86") && !cfg!(target_arch = "x86_64") {
        return false;
    }

    if let Ok(output) = Command::new("ldconfig").arg("-p").output() {
        let libs = String::from_utf8_lossy(&output.stdout);
        if libs.contains("libmfx") || libs.contains("libvpl") {
            info!("Found Intel QSV runtime on Linux");
            return true;
        }
    }

    debug!("Intel QSV runtime not found on Linux");
    false
}

/// Cached QSV availability check (only check once at startup)
static QSV_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn check_qsv_available() -> bool {
    *QSV_AVAILABLE.get_or_init(|| {
        let available = is_qsv_runtime_available();
        if available {
            info!("Intel QuickSync Video (QSV) runtime detected - QSV decoding enabled");
        } else {
            info!("Intel QSV runtime not detected - QSV decoding disabled (install Intel GPU drivers for QSV support)");
        }
        available
    })
}

/// Cached Intel GPU name for QSV capability detection
static INTEL_GPU_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Get the Intel GPU name from wgpu adapter info
fn get_intel_gpu_name() -> String {
    INTEL_GPU_NAME
        .get_or_init(|| {
            pollster::block_on(async {
                let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
                let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;

                for adapter in adapters {
                    let info = adapter.get_info();
                    let name = info.name.to_lowercase();
                    if name.contains("intel") {
                        return info.name.clone();
                    }
                }
                String::new()
            })
        })
        .clone()
}

// Note: QSV codec checking removed - macOS now uses GStreamer with VideoToolbox
// The is_qsv_supported_for_codec function was only used for FFmpeg-based decoding

/// Cached supported decoder backends
static SUPPORTED_BACKENDS: std::sync::OnceLock<Vec<VideoDecoderBackend>> =
    std::sync::OnceLock::new();

/// Get list of supported decoder backends for the current system
pub fn get_supported_decoder_backends() -> Vec<VideoDecoderBackend> {
    SUPPORTED_BACKENDS
        .get_or_init(|| {
            let mut backends = vec![VideoDecoderBackend::Auto];

            // Always check what's actually available
            #[cfg(target_os = "macos")]
            {
                backends.push(VideoDecoderBackend::VideoToolbox);
            }

            #[cfg(target_os = "windows")]
            {
                let gpu = detect_gpu_vendor();
                let qsv = check_qsv_available();

                // GStreamer D3D11 decoder - supports both H.264 and HEVC
                // This is the recommended decoder for Windows (stable, works on all GPUs)
                backends.push(VideoDecoderBackend::Dxva);

                // GPU-specific accelerators
                if gpu == GpuVendor::Nvidia {
                    backends.push(VideoDecoderBackend::Cuvid);
                }

                if qsv || gpu == GpuVendor::Intel {
                    backends.push(VideoDecoderBackend::Qsv);
                }
            }

            #[cfg(target_os = "linux")]
            {
                let gpu = detect_gpu_vendor();
                let qsv = check_qsv_available();

                // GStreamer with hardware acceleration is the preferred decoder on Linux
                // It automatically selects the best available hardware decoder (VAAPI, NVDEC, etc.)
                if super::gstreamer_decoder::is_gstreamer_available() {
                    backends.push(VideoDecoderBackend::VulkanVideo); // GStreamer-based hardware decode
                }

                if gpu == GpuVendor::Nvidia {
                    backends.push(VideoDecoderBackend::Cuvid);
                }

                if qsv || gpu == GpuVendor::Intel {
                    backends.push(VideoDecoderBackend::Qsv);
                }

                // VAAPI is generally available on Linux (AMD/Intel)
                backends.push(VideoDecoderBackend::Vaapi);
            }

            backends.push(VideoDecoderBackend::Software);
            backends
        })
        .clone()
}

/// Commands sent to the decoder thread
enum DecoderCommand {
    /// Decode a packet and return result via channel (blocking mode)
    Decode(Vec<u8>),
    /// Decode a packet and write directly to SharedFrame (non-blocking mode)
    DecodeAsync {
        data: Vec<u8>,
        receive_time: std::time::Instant,
    },
    Stop,
}

/// Stats from the decoder thread
#[derive(Debug, Clone)]
pub struct DecodeStats {
    /// Time from packet receive to decode complete (ms)
    pub decode_time_ms: f32,
    /// Whether a frame was produced
    pub frame_produced: bool,
    /// Whether a keyframe is needed (too many consecutive decode failures)
    pub needs_keyframe: bool,
}

/// Video decoder using FFmpeg with hardware acceleration
/// Uses a dedicated thread for decoding since FFmpeg types are not Send
pub struct VideoDecoder {
    cmd_tx: mpsc::Sender<DecoderCommand>,
    frame_rx: mpsc::Receiver<Option<VideoFrame>>,
    /// Stats receiver for non-blocking mode
    stats_rx: Option<tokio_mpsc::Receiver<DecodeStats>>,
    hw_accel: bool,
    frames_decoded: u64,
    /// SharedFrame for non-blocking writes (set via set_shared_frame)
    shared_frame: Option<Arc<SharedFrame>>,
}

impl VideoDecoder {
    // Note: VideoDecoder::new() removed for macOS - macOS now uses GStreamer via UnifiedVideoDecoder
    
    /// Create a new video decoder configured for non-blocking async mode
    /// Decoded frames are written directly to the SharedFrame
    pub fn new_async(
        codec: VideoCodec,
        backend: VideoDecoderBackend,
        shared_frame: Arc<SharedFrame>,
    ) -> Result<(Self, tokio_mpsc::Receiver<DecodeStats>)> {
        // On Windows, use native DXVA decoder (no FFmpeg)
        // This uses D3D11 Video API directly for hardware acceleration
        #[cfg(target_os = "windows")]
        {
            return Err(anyhow!(
                "VideoDecoder::new_async not supported on Windows. Use UnifiedVideoDecoder::new_async instead."
            ));
        }

        // On Linux, use GStreamer for hardware-accelerated decoding
        // GStreamer automatically selects the best available backend (VAAPI, NVDEC, V4L2, etc.)
        #[cfg(target_os = "linux")]
        {
            // Use GStreamer decoder (auto-selects V4L2/VAAPI/NVDEC/software)
            // The GStreamer decoder automatically selects the best available backend
            if super::gstreamer_decoder::is_gstreamer_available() {
                info!(
                    "Using GStreamer decoder for {:?} (auto-selects V4L2/VA/VAAPI/software)",
                    codec
                );

                let gst_codec = match codec {
                    VideoCodec::H264 => super::gstreamer_decoder::GstCodec::H264,
                    VideoCodec::H265 => super::gstreamer_decoder::GstCodec::H265,
                    VideoCodec::AV1 => super::gstreamer_decoder::GstCodec::AV1,
                };

                let config = super::gstreamer_decoder::GstDecoderConfig {
                    codec: gst_codec,
                    width: 1920,
                    height: 1080,
                    low_latency: true, // Enable low latency for streaming
                };

                let gst_decoder = super::gstreamer_decoder::GStreamerDecoder::new(config)
                    .map_err(|e| anyhow!("Failed to create GStreamer decoder: {}", e))?;

                info!("GStreamer decoder created successfully!");

                let (cmd_tx, cmd_rx) = mpsc::channel::<DecoderCommand>();
                let (frame_tx, frame_rx) = mpsc::channel::<Option<VideoFrame>>();
                let (stats_tx, stats_rx) = tokio_mpsc::channel::<DecodeStats>(64);

                let shared_frame_clone = shared_frame.clone();

                thread::spawn(move || {
                    info!("GStreamer decoder thread started");
                    let mut decoder = gst_decoder;
                    let mut frames_decoded = 0u64;
                    let mut consecutive_failures = 0u32;
                    // WiFi users may experience packet loss causing temporary decode failures.
                    // Threshold of 5 balances between quick recovery after focus loss and 
                    // tolerance for transient WiFi packet loss (avoids green screen flashes).
                    // At 120fps, 5 failures = ~42ms of tolerance before requesting keyframe.
                    const KEYFRAME_REQUEST_THRESHOLD: u32 = 5;
                    const FRAMES_TO_SKIP: u64 = 5;

                    while let Ok(cmd) = cmd_rx.recv() {
                        match cmd {
                            DecoderCommand::Decode(data) => {
                                let result = decoder.decode(&data);
                                let _ = frame_tx.send(result.ok().flatten());
                            }
                            DecoderCommand::DecodeAsync { data, receive_time } => {
                                let result = decoder.decode(&data);
                                let decode_time_ms = receive_time.elapsed().as_secs_f32() * 1000.0;

                                let frame_produced = matches!(&result, Ok(Some(_)));

                                let needs_keyframe = if frame_produced {
                                    consecutive_failures = 0;
                                    false
                                } else {
                                    consecutive_failures += 1;
                                    consecutive_failures == KEYFRAME_REQUEST_THRESHOLD
                                };

                                if let Ok(Some(frame)) = result {
                                    frames_decoded += 1;
                                    if frames_decoded > FRAMES_TO_SKIP {
                                        shared_frame_clone.write(frame);
                                    }
                                }

                                let _ = stats_tx.try_send(DecodeStats {
                                    decode_time_ms,
                                    frame_produced,
                                    needs_keyframe,
                                });
                            }
                            DecoderCommand::Stop => break,
                        }
                    }
                    info!("GStreamer decoder thread stopped");
                });

                let decoder = Self {
                    cmd_tx,
                    frame_rx,
                    stats_rx: None,
                    hw_accel: true,
                    frames_decoded: 0,
                    shared_frame: Some(shared_frame),
                };

                return Ok((decoder, stats_rx));
            }

            // No decoder available
            return Err(anyhow!(
                "No video decoder available on Linux. Requires either:\n\
                 - Vulkan Video support (Intel Arc, NVIDIA RTX, AMD RDNA2+)\n\
                 - GStreamer with hardware decoding:\n\
                   * V4L2 (Raspberry Pi / embedded)\n\
                   * VA plugin (Intel/AMD desktop - vah264dec)\n\
                   * VAAPI plugin (legacy Intel/AMD - vaapih264dec)\n\
                   * Software fallback (avdec_h264)\n\
                 Run 'vulkaninfo | grep video' to check Vulkan Video support.\n\
                 Run 'gst-inspect-1.0 | grep -E \"v4l2|va|vaapi|avdec\"' to check GStreamer decoders."
            ));
        }

        // Note: macOS FFmpeg path removed - macOS now uses GStreamer via UnifiedVideoDecoder
        #[cfg(target_os = "macos")]
        {
            return Err(anyhow!(
                "VideoDecoder::new_async not supported on macOS. Use UnifiedVideoDecoder::new_async instead."
            ));
        }
    }

    // Note: All FFmpeg-based decoder code removed for macOS
    // macOS now uses GStreamer with VideoToolbox via UnifiedVideoDecoder
    // The following methods (decode, decode_async, etc.) are still used by Linux GStreamer path


    /// Decode a NAL unit - sends to decoder thread and receives result
    /// WARNING: This is BLOCKING and will stall the calling thread!
    /// For low-latency streaming, use `decode_async()` instead.
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<VideoFrame>> {
        // Send decode command
        self.cmd_tx
            .send(DecoderCommand::Decode(data.to_vec()))
            .map_err(|_| anyhow!("Decoder thread closed"))?;

        // Receive result (blocking)
        match self.frame_rx.recv() {
            Ok(frame) => {
                if frame.is_some() {
                    self.frames_decoded += 1;
                }
                Ok(frame)
            }
            Err(_) => Err(anyhow!("Decoder thread closed")),
        }
    }

    /// Decode a NAL unit asynchronously - fire and forget
    /// The decoded frame will be written directly to the SharedFrame.
    /// Stats are sent via the stats channel returned from `new_async()`.
    ///
    /// This method NEVER blocks the calling thread, making it ideal for
    /// the main streaming loop where input responsiveness is critical.
    pub fn decode_async(&mut self, data: &[u8], receive_time: std::time::Instant) -> Result<()> {
        self.cmd_tx
            .send(DecoderCommand::DecodeAsync {
                data: data.to_vec(),
                receive_time,
            })
            .map_err(|_| anyhow!("Decoder thread closed"))?;

        self.frames_decoded += 1; // Optimistic count
        Ok(())
    }

    /// Check if using hardware acceleration
    pub fn is_hw_accelerated(&self) -> bool {
        self.hw_accel
    }

    /// Get number of frames decoded
    pub fn frames_decoded(&self) -> u64 {
        self.frames_decoded
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        // Signal decoder thread to stop
        let _ = self.cmd_tx.send(DecoderCommand::Stop);
    }
}

// ============================================================================
// Unified Video Decoder - Wraps FFmpeg or Native DXVA decoder
// ============================================================================

/// Unified video decoder that uses GStreamer backend across platforms
///
/// This enum provides a common interface for decoder types, allowing
/// the streaming code to use the appropriate backend transparently.
/// - Windows x64: GStreamer D3D11 for all codecs (H.264/H.265/AV1)
/// - Windows ARM64: Not supported (GStreamer ARM64 binaries not available)
/// - macOS: GStreamer with VideoToolbox via vtdec (H.264/H.265/AV1)
/// - Linux: GStreamer with V4L2/VA-API/software fallback
#[cfg(all(windows, target_arch = "x86_64"))]
pub enum UnifiedVideoDecoder {
    /// GStreamer D3D11 decoder (H.264/H.265/AV1, with hardware acceleration)
    GStreamer(GStreamerDecoderWrapper),
}

/// Wrapper for GStreamer decoder with async interface (Windows x64 and macOS)
#[cfg(any(all(windows, target_arch = "x86_64"), target_os = "macos"))]
pub struct GStreamerDecoderWrapper {
    decoder: super::gstreamer_decoder::GStreamerDecoder,
    shared_frame: Arc<SharedFrame>,
    stats_tx: tokio_mpsc::Sender<DecodeStats>,
    frames_decoded: u64,
    /// Track consecutive failures for keyframe request
    consecutive_failures: u32,
}

/// Windows ARM64: Video decoding not supported (no GStreamer ARM64 binaries available)
/// This is a placeholder that will return an error when attempting to create a decoder
#[cfg(all(windows, target_arch = "aarch64"))]
pub enum UnifiedVideoDecoder {
    /// Placeholder - will never be instantiated
    #[allow(dead_code)]
    Unsupported,
}

#[cfg(target_os = "macos")]
pub enum UnifiedVideoDecoder {
    /// GStreamer with VideoToolbox decoder (H.264/H.265/AV1)
    GStreamer(GStreamerDecoderWrapper),
}

#[cfg(target_os = "linux")]
pub enum UnifiedVideoDecoder {
    /// Linux uses GStreamer (V4L2/VA-API/software)
    Ffmpeg(VideoDecoder),
}

impl UnifiedVideoDecoder {
    /// Create a new unified decoder with the specified backend
    pub fn new_async(
        codec: VideoCodec,
        backend: VideoDecoderBackend,
        shared_frame: Arc<SharedFrame>,
    ) -> Result<(Self, tokio_mpsc::Receiver<DecodeStats>)> {
        // Windows x64: Use GStreamer D3D11 for all codecs (H.264/H.265/AV1)
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            // Suppress unused variable warning - backend is used on other platforms
            let _ = backend;

            // Use GStreamer D3D11 decoder for all codecs
            // This is stable and supports H.264, H.265, and AV1 with hardware acceleration
            let gst_codec = match codec {
                VideoCodec::H264 => {
                    info!("Creating GStreamer D3D11 decoder for H.264");
                    super::gstreamer_decoder::GstCodec::H264
                }
                VideoCodec::H265 => {
                    info!("Creating GStreamer D3D11 decoder for H.265");
                    super::gstreamer_decoder::GstCodec::H265
                }
                VideoCodec::AV1 => {
                    info!("Creating GStreamer D3D11 decoder for AV1");
                    super::gstreamer_decoder::GstCodec::AV1
                }
            };

            let gst_config = super::gstreamer_decoder::GstDecoderConfig {
                codec: gst_codec,
                width: 1920,
                height: 1080,
                low_latency: true,
            };

            let gst_decoder = super::gstreamer_decoder::GStreamerDecoder::new(gst_config)
                .map_err(|e| anyhow!("Failed to create GStreamer {:?} decoder: {}", codec, e))?;

            info!("GStreamer D3D11 {:?} decoder created successfully", codec);

            let (stats_tx, stats_rx) = tokio_mpsc::channel::<DecodeStats>(64);

            let wrapper = GStreamerDecoderWrapper {
                decoder: gst_decoder,
                shared_frame: shared_frame.clone(),
                stats_tx,
                frames_decoded: 0,
                consecutive_failures: 0,
            };

            return Ok((UnifiedVideoDecoder::GStreamer(wrapper), stats_rx));
        }

        // Windows ARM64: Video decoding not supported
        // GStreamer ARM64 binaries are not available
        #[cfg(all(windows, target_arch = "aarch64"))]
        {
            // Suppress unused variable warnings
            let _ = (codec, backend, shared_frame);
            return Err(anyhow!(
                "Video decoding is not supported on Windows ARM64. \
                 GStreamer ARM64 binaries are not available. \
                 Please use Windows x64, macOS, or Linux instead."
            ));
        }

        // macOS: Use GStreamer with VideoToolbox (vtdec)
        #[cfg(target_os = "macos")]
        {
            // Suppress unused variable warning - backend is used on other platforms
            let _ = backend;

            // Use GStreamer with VideoToolbox for hardware acceleration
            let gst_codec = match codec {
                VideoCodec::H264 => {
                    info!("Creating GStreamer VideoToolbox decoder for H.264");
                    super::gstreamer_decoder::GstCodec::H264
                }
                VideoCodec::H265 => {
                    info!("Creating GStreamer VideoToolbox decoder for H.265");
                    super::gstreamer_decoder::GstCodec::H265
                }
                VideoCodec::AV1 => {
                    info!("Creating GStreamer VideoToolbox decoder for AV1");
                    super::gstreamer_decoder::GstCodec::AV1
                }
            };

            let gst_config = super::gstreamer_decoder::GstDecoderConfig {
                codec: gst_codec,
                width: 1920,
                height: 1080,
                low_latency: true,
            };

            let gst_decoder = super::gstreamer_decoder::GStreamerDecoder::new(gst_config)
                .map_err(|e| anyhow!("Failed to create GStreamer {:?} decoder: {}", codec, e))?;

            info!("GStreamer VideoToolbox {:?} decoder created successfully", codec);

            let (stats_tx, stats_rx) = tokio_mpsc::channel::<DecodeStats>(64);

            let wrapper = GStreamerDecoderWrapper {
                decoder: gst_decoder,
                shared_frame: shared_frame.clone(),
                stats_tx,
                frames_decoded: 0,
                consecutive_failures: 0,
            };

            return Ok((UnifiedVideoDecoder::GStreamer(wrapper), stats_rx));
        }

        // Linux: Use FFmpeg/GStreamer decoder
        #[cfg(target_os = "linux")]
        {
            let (ffmpeg_decoder, stats_rx) = VideoDecoder::new_async(codec, backend, shared_frame)?;
            Ok((UnifiedVideoDecoder::Ffmpeg(ffmpeg_decoder), stats_rx))
        }
    }

    /// Decode a frame asynchronously
    pub fn decode_async(&mut self, data: &[u8], receive_time: std::time::Instant) -> Result<()> {
        match self {
            #[cfg(target_os = "linux")]
            UnifiedVideoDecoder::Ffmpeg(decoder) => decoder.decode_async(data, receive_time),
            #[cfg(any(all(windows, target_arch = "x86_64"), target_os = "macos"))]
            UnifiedVideoDecoder::GStreamer(wrapper) => {
                wrapper.decode_async(data, receive_time);
                Ok(())
            }
            #[cfg(all(windows, target_arch = "aarch64"))]
            UnifiedVideoDecoder::Unsupported => {
                Err(anyhow!("Video decoding not supported on Windows ARM64"))
            }
        }
    }

    /// Check if using hardware acceleration
    pub fn is_hw_accelerated(&self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            UnifiedVideoDecoder::Ffmpeg(decoder) => decoder.is_hw_accelerated(),
            #[cfg(any(all(windows, target_arch = "x86_64"), target_os = "macos"))]
            UnifiedVideoDecoder::GStreamer(_) => true, // GStreamer uses hardware acceleration
            #[cfg(all(windows, target_arch = "aarch64"))]
            UnifiedVideoDecoder::Unsupported => false,
        }
    }

    /// Get number of frames decoded
    pub fn frames_decoded(&self) -> u64 {
        match self {
            #[cfg(target_os = "linux")]
            UnifiedVideoDecoder::Ffmpeg(decoder) => decoder.frames_decoded(),
            #[cfg(any(all(windows, target_arch = "x86_64"), target_os = "macos"))]
            UnifiedVideoDecoder::GStreamer(wrapper) => wrapper.frames_decoded,
            #[cfg(all(windows, target_arch = "aarch64"))]
            UnifiedVideoDecoder::Unsupported => 0,
        }
    }
}

#[cfg(any(all(windows, target_arch = "x86_64"), target_os = "macos"))]
impl GStreamerDecoderWrapper {
    /// Threshold for requesting a keyframe after consecutive failures (lowered for faster recovery)
    const KEYFRAME_REQUEST_THRESHOLD: u32 = 3;

    /// Decode a frame asynchronously and write to SharedFrame
    pub fn decode_async(&mut self, data: &[u8], receive_time: std::time::Instant) {
        let decode_start = std::time::Instant::now();

        match self.decoder.decode(data) {
            Ok(Some(frame)) => {
                self.frames_decoded += 1;
                self.consecutive_failures = 0;
                self.shared_frame.write(frame);

                // Measure decode time from when we started pushing data
                let decode_time_ms = decode_start.elapsed().as_secs_f32() * 1000.0;

                // Log first frame
                if self.frames_decoded == 1 {
                    info!(
                        "GStreamer: First frame decoded in {:.1}ms (pipeline latency: {:.1}ms)",
                        decode_time_ms,
                        receive_time.elapsed().as_secs_f32() * 1000.0
                    );
                }

                let _ = self.stats_tx.try_send(DecodeStats {
                    decode_time_ms,
                    frame_produced: true,
                    needs_keyframe: false,
                });
            }
            Ok(None) => {
                // No frame produced yet (buffering or B-frame reordering)
                self.consecutive_failures += 1;

                let needs_keyframe =
                    if self.consecutive_failures == Self::KEYFRAME_REQUEST_THRESHOLD {
                        warn!(
                            "GStreamer: {} consecutive packets without frame - requesting keyframe",
                            self.consecutive_failures
                        );
                        true
                    } else if self.consecutive_failures > Self::KEYFRAME_REQUEST_THRESHOLD
                        && self.consecutive_failures % 20 == 0
                    {
                        warn!(
                            "GStreamer: Still failing after {} packets - requesting keyframe again",
                            self.consecutive_failures
                        );
                        true
                    } else {
                        false
                    };

                let decode_time_ms = decode_start.elapsed().as_secs_f32() * 1000.0;
                let _ = self.stats_tx.try_send(DecodeStats {
                    decode_time_ms,
                    frame_produced: false,
                    needs_keyframe,
                });
            }
            Err(e) => {
                warn!("GStreamer decode error: {}", e);
                self.consecutive_failures += 1;

                let decode_time_ms = decode_start.elapsed().as_secs_f32() * 1000.0;
                let _ = self.stats_tx.try_send(DecodeStats {
                    decode_time_ms,
                    frame_produced: false,
                    needs_keyframe: self.consecutive_failures >= Self::KEYFRAME_REQUEST_THRESHOLD,
                });
            }
        }
    }
}
