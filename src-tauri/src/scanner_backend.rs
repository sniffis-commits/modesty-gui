use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::ffi::CString;
use std::os::raw::c_char;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScannerBackend {
    Python,
    RustCli,
    RustCdyLib,
}

impl Default for ScannerBackend {
    fn default() -> Self {
        ScannerBackend::Python
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub backend: ScannerBackend,
    pub model_dir: Option<String>,
    pub python_path: Option<String>,
    pub rust_cli_path: Option<String>,
    pub lib_path: Option<String>,
    pub auto_download_models: bool,
    pub model_base_url: Option<String>,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            backend: ScannerBackend::Python,
            model_dir: None,
            python_path: None,
            rust_cli_path: None,
            lib_path: None,
            auto_download_models: true,
            model_base_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOptions {
    pub fps: f64,
    pub every_frame: bool,
    pub max_frames: u32,
    pub details: bool,
    pub debug: bool,
    pub subtitles: bool,
    pub tiles: String,
    pub hwaccel: String,
    pub frames: String,
    pub decoder: String,
    pub face_crop: bool,
    pub save_samples: bool,
    pub include_frame_ratings: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            fps: 1.0,
            every_frame: false,
            max_frames: 2000,
            details: true,
            debug: false,
            subtitles: true,
            tiles: "off".to_string(),
            hwaccel: "none".to_string(),
            frames: "raw".to_string(),
            decoder: "auto".to_string(),
            face_crop: false,
            save_samples: true,
            include_frame_ratings: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ScanResult {
    pub rating: Option<u8>,
    pub verdict: Option<String>,
    pub phrase: Option<String>,
    pub nsfw_score: Option<f32>,
    pub female_score: Option<f32>,
    pub male_score: Option<f32>,
    pub gender: Option<String>,
    pub classes: Option<serde_json::Value>,
    pub decided_by: Option<String>,
    pub frames: Option<u32>,
    pub problem_count: Option<u32>,
    pub scored_frames: Option<u32>,
    pub romance_frames: Option<u32>,
    pub worst_score: Option<f32>,
    pub duration_sec: Option<f64>,
    pub synthetic: Option<bool>,
    pub saved_samples: Option<Vec<String>>,
    pub frame_timeline: Option<Vec<FrameTimelineEntry>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FrameTimelineEntry {
    pub frame: String,
    pub rating: u8,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanProgress {
    Progress { frame: u32, total: u32, rating: u8 },
    FrameDone { frame: u32, rating: u8 },
    Done,
    Error { message: String },
}

pub struct Scanner {
    config: ScannerConfig,
}

impl Scanner {
    pub fn new(config: ScannerConfig) -> Self {
        Self { config }
    }

    pub fn from_env() -> Self {
        let config = ScannerConfig {
            backend: std::env::var("NSFW_SCANNER_BACKEND")
                .ok()
                .and_then(|s| match s.as_str() {
                    "python" => Some(ScannerBackend::Python),
                    "rust-cli" => Some(ScannerBackend::RustCli),
                    "rust-cdylib" => Some(ScannerBackend::RustCdyLib),
                    _ => None,
                })
                .unwrap_or_default(),
            model_dir: std::env::var("NSFW_MODEL_DIR").ok(),
            python_path: std::env::var("PYTHON_PATH").ok(),
            rust_cli_path: std::env::var("NSFW_RUST_CLI_PATH").ok(),
            lib_path: std::env::var("NSFW_LIB_PATH").ok(),
            auto_download_models: std::env::var("NSFW_AUTO_DOWNLOAD")
                .map(|v| v != "0" && v != "false")
                .unwrap_or(true),
            model_base_url: std::env::var("NSFW_MODEL_BASE_URL").ok(),
        };
        Self::new(config)
    }

    pub fn scan_image(&self, path: &Path) -> Result<ScanResult, String> {
        match self.config.backend {
            ScannerBackend::Python => self.scan_python(path),
            ScannerBackend::RustCli => self.scan_rust_cli(path),
            ScannerBackend::RustCdyLib => self.scan_cdylib(path),
        }
    }

    pub fn scan_video(
        &self,
        path: &Path,
        options: ScanOptions,
        progress_tx: Option<tokio::sync::mpsc::Sender<ScanProgress>>,
    ) -> Result<ScanResult, String> {
        match self.config.backend {
            ScannerBackend::Python => self.scan_video_python(path, options, progress_tx),
            ScannerBackend::RustCli => self.scan_video_rust_cli(path, options, progress_tx),
            ScannerBackend::RustCdyLib => self.scan_video_cdylib(path, options, progress_tx),
        }
    }

    fn scan_python(&self, path: &Path) -> Result<ScanResult, String> {
        let library_path = self.get_library_path()?;
        let model_dir = self.config.model_dir.as_deref().unwrap_or("");

        let script = format!(
            r#"
import sys, json, os
sys.path.insert(0, r"{}")
from nsfw_scanner import analyze_pil_image, load_nsfw_model, load_gender_model
from PIL import Image

model_dir = r"{}"
nsfw_session = load_nsfw_model()
gender_session = load_gender_model()
img = Image.open(r"{}").convert("RGB")
result = analyze_pil_image(img, nsfw_session, gender_session=gender_session, details=True)
print(json.dumps(result))
"#,
            library_path.to_string_lossy(),
            model_dir,
            path.to_string_lossy()
        );

        let output = Command::new(self.config.python_path.as_deref().unwrap_or("python"))
            .arg("-c")
            .arg(&script)
            .env("PYTHONPATH", self.get_library_path()?)
            .output()
            .map_err(|e| format!("Failed to run Python: {}", e))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let result: ScanResult = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse Python output: {}", e))?;
        Ok(result)
    }

    fn scan_video_python(
        &self,
        path: &Path,
        options: ScanOptions,
        progress_tx: Option<tokio::sync::mpsc::Sender<ScanProgress>>,
    ) -> Result<ScanResult, String> {
        let library_path = self.get_library_path()?;
        let bridge_path = self.write_bridge_script()?;

        let mut cmd = Command::new(self.config.python_path.as_deref().unwrap_or("python"));
        cmd.arg(&bridge_path)
            .arg(path.to_string_lossy().to_string())
            .arg(serde_json::to_string(&options).unwrap())
            .env("PYTHONPATH", library_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn Python: {}", e))?;

        let stdout = child.stdout.take().ok_or("No stdout")?;
        let stderr = child.stderr.take().ok_or("No stderr")?;

        let mut result = None;
        for line in BufReader::new(stdout).lines() {
            let line = line.map_err(|e| e.to_string())?;
            if let Ok(msg) = serde_json::from_str::<ScanBridgeMessage>(&line) {
                match msg.kind.as_str() {
                    "progress" => {
                        if let Some(tx) = &progress_tx {
                            let _ = tx.blocking_send(ScanProgress::Progress {
                                frame: msg.data["frame"].as_u64().unwrap_or(0) as u32,
                                total: msg.data["total"].as_u64().unwrap_or(0) as u32,
                                rating: msg.data["rating"].as_u64().unwrap_or(0) as u8,
                            });
                        }
                    }
                    "result" => result = Some(msg.data),
                    _ => {}
                }
            }
        }

        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Python scanner failed".to_string());
        }

        let result = result.ok_or_else(|| "No result from Python scanner".to_string())?;
        serde_json::from_value(result).map_err(|e| format!("Failed to parse Python result: {}", e))
    }

    fn write_bridge_script(&self) -> Result<PathBuf, String> {
        const BRIDGE: &str = include_str!("scripts/scan_video_bridge.py");
        let tmp = std::env::temp_dir().join(format!("nsfw_bridge_{}.py", uuid::Uuid::new_v4()));
        std::fs::write(&tmp, BRIDGE)
            .map_err(|e| format!("Failed to write bridge: {}", e))?;
        Ok(tmp)
    }

    fn scan_rust_cli(&self, path: &Path) -> Result<ScanResult, String> {
        let cli_path = self.get_rust_cli_path()?;
        let model_dir = self.config.model_dir.as_deref().unwrap_or("");

        let output = Command::new(&cli_path)
            .arg(path)
            .arg("--json")
            .arg("--details")
            .arg("--model-dir")
            .arg(model_dir)
            .output()
            .map_err(|e| format!("Failed to run Rust CLI: {}", e))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let result: ScanResult = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse Rust CLI output: {}", e))?;
        Ok(result)
    }

    fn scan_video_rust_cli(
        &self,
        path: &Path,
        options: ScanOptions,
        progress_tx: Option<tokio::sync::mpsc::Sender<ScanProgress>>,
    ) -> Result<ScanResult, String> {
        self.scan_video_frames(path, options, progress_tx)
    }

    #[cfg(feature = "cdylib")]
    fn scan_cdylib(&self, path: &Path) -> Result<ScanResult, String> {
        use libloading::{Library, Symbol};
        use std::os::raw::c_char;
        use std::ffi::CString;

        let lib_path = self.get_lib_path()?;
        let lib = unsafe { Library::new(&lib_path) }
            .map_err(|e| format!("Failed to load library: {}", e))?;

        let create_session: Symbol<unsafe extern "C" fn(*const c_char) -> *mut std::ffi::c_void> = unsafe {
            lib.get(b"nsfw_lib_rust_session_create\0").map_err(|e| e.to_string())?
        };
        let scan_file: Symbol<unsafe extern "C" fn(*mut std::ffi::c_void, *const c_char) -> *mut c_char> = unsafe {
            lib.get(b"nsfw_lib_rust_scan_file\0").map_err(|e| e.to_string())?
        };
        let string_free: Symbol<unsafe extern "C" fn(*mut c_char)> = unsafe {
            lib.get(b"nsfw_lib_rust_string_free\0").map_err(|e| e.to_string())?
        };
        let session_free: Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)> = unsafe {
            lib.get(b"nsfw_lib_rust_session_free\0").map_err(|e| e.to_string())?
        };

        let model_dir = self.config.model_dir.as_deref().unwrap_or("");
        let model_dir_c = CString::new(model_dir).unwrap();

        let session = unsafe { create_session(model_dir_c.as_ptr()) };
        if session.is_null() {
            return Err("Failed to create scanner session".to_string());
        }

        let path_c = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        let result_ptr = unsafe { scan_file(session, path_c.as_ptr()) };

        if result_ptr.is_null() {
            unsafe { session_free(session) };
            return Err("Scan failed".to_string());
        }

        let json = unsafe { CString::from_raw(result_ptr).to_string_lossy().to_string() };
        unsafe { string_free(result_ptr) };
        unsafe { session_free(session) };

        let result: ScanResult = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse cdylib output: {}", e))?;
        Ok(result)
    }

    #[cfg(not(feature = "cdylib"))]
    fn scan_cdylib(&self, _path: &Path) -> Result<ScanResult, String> {
        Err("cdylib feature not enabled".to_string())
    }

    fn scan_video_cdylib(
        &self,
        path: &Path,
        options: ScanOptions,
        progress_tx: Option<tokio::sync::mpsc::Sender<ScanProgress>>,
    ) -> Result<ScanResult, String> {
        self.scan_video_frames(path, options, progress_tx)
    }

    fn scan_video_frames(
        &self,
        path: &Path,
        options: ScanOptions,
        progress_tx: Option<tokio::sync::mpsc::Sender<ScanProgress>>,
    ) -> Result<ScanResult, String> {
        let frames_dir = std::env::temp_dir().join(format!("nsfw_frames_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&frames_dir).map_err(|e| e.to_string())?;

        let fps = options.fps;
        let max_frames = options.max_frames;

        let output = Command::new("ffmpeg")
            .args([
                "-i", path.to_str().unwrap(),
                "-vf", &format!("fps={}", fps),
                "-vframes", &max_frames.to_string(),
                "-vsync", "vfr",
                frames_dir.join("frame_%06d.jpg").to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("ffmpeg failed: {}", e))?;

        if !output.status.success() {
            return Err(format!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        let mut frames = Vec::new();
        for entry in std::fs::read_dir(&frames_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("jpg") {
                frames.push(entry.path());
            }
        }
        frames.sort();
        frames.truncate(max_frames as usize);

        if let Some(tx) = &progress_tx {
            let _ = tx.blocking_send(ScanProgress::Progress { frame: 0, total: frames.len() as u32, rating: 0 });
        }

        let mut worst_rating = 0u8;
        let mut frame_results = Vec::new();

        for (i, frame_path) in frames.iter().enumerate() {
            let result = match self.config.backend {
                ScannerBackend::RustCli => self.scan_rust_cli(frame_path)?,
                ScannerBackend::RustCdyLib => self.scan_cdylib(frame_path)?,
                _ => return Err("Video scanning only supported with Rust backends".to_string()),
            };

            let rating = result.rating.unwrap_or(0);
            worst_rating = worst_rating.max(rating);

            frame_results.push(FrameTimelineEntry {
                frame: frame_path.file_name().unwrap().to_string_lossy().to_string(),
                rating,
                action: if rating >= 5 { "skip" } else { "allow" }.to_string(),
            });

            if let Some(tx) = &progress_tx {
                let _ = tx.blocking_send(ScanProgress::FrameDone { frame: i as u32, rating });
            }
        }

        let _ = std::fs::remove_dir_all(&frames_dir);

        let final_rating = if worst_rating >= 10 { 10 }
            else if worst_rating >= 7 { 7 }
            else if worst_rating >= 5 { 5 }
            else if worst_rating >= 1 { 1 }
            else { 0 };

        Ok(ScanResult {
            rating: Some(final_rating),
            verdict: Some(match final_rating {
                10 => "explicit",
                7 => "revealing",
                6 => "romance",
                5 => "women/girls",
                1 => "no-unsafe-signal",
                _ => "not-assessed",
            }.to_string()),
            phrase: Some(match final_rating {
                10 => "explicit adult content",
                7 => "revealing, not fully explicit",
                6 => "romance - clothed, affectionate",
                5 => "women/girls present, no stronger unsafe signal detected",
                1 => "no unsafe signal detected",
                _ => "not assessed",
            }.to_string()),
            problem_count: Some(frame_results.iter().filter(|f| f.rating >= 5).count() as u32),
            frame_timeline: Some(frame_results),
            frames: Some(frames.len() as u32),
            scored_frames: Some(frames.len() as u32),
            ..Default::default()
        })
    }

    fn get_library_path(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.config.lib_path {
            let p = PathBuf::from(p);
            if p.exists() { return Ok(p); }
        }

        let executable = std::env::current_exe()
            .map_err(|e| format!("Failed to locate executable: {}", e))?;
        let exe_dir = executable.parent().ok_or("No parent dir")?;

        let bundled = exe_dir.join("nsfw-lib").join("src");
        if bundled.join("nsfw_scanner").is_dir() {
            return Ok(bundled);
        }

        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..").join("..").join("nsfw-lib").join("src");
        if dev.join("nsfw_scanner").is_dir() {
            return Ok(dev);
        }

        Err("Scanner library not found".to_string())
    }

    fn get_rust_cli_path(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.config.rust_cli_path {
            let p = PathBuf::from(p);
            if p.exists() { return Ok(p); }
        }

        let executable = std::env::current_exe()
            .map_err(|e| format!("Failed to locate executable: {}", e))?;
        let exe_dir = executable.parent().ok_or("No parent dir")?;

        let bundled = exe_dir.join("nsfw-scan-rust.exe");
        if bundled.exists() {
            return Ok(bundled);
        }

        Err("nsfw-scan-rust.exe not found".to_string())
    }

    fn get_lib_path(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.config.lib_path {
            let p = PathBuf::from(p);
            if p.exists() { return Ok(p); }
        }

        let executable = std::env::current_exe()
            .map_err(|e| format!("Failed to locate executable: {}", e))?;
        let exe_dir = executable.parent().ok_or("No parent dir")?;

        #[cfg(windows)]
        let name = "nsfw_lib_rust.dll";
        #[cfg(target_os = "macos")]
        let name = "libnsfw_lib_rust.dylib";
        #[cfg(all(unix, not(target_os = "macos")))]
        let name = "libnsfw_lib_rust.so";

        let bundled = exe_dir.join(name);
        if bundled.exists() {
            return Ok(bundled);
        }

        Err(format!("{} not found", name))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanBridgeMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: serde_json::Value,
}