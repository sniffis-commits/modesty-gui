use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::Emitter;

use crate::scanner_backend::{Scanner, ScannerBackend, ScannerConfig, ScanOptions, ScanProgress, ScanResult, ScanBridgeMessage};

fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn scanner_library_path() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Failed to locate the GUI executable: {}", error))?;
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "GUI executable has no parent directory".to_string())?;
    let bundled = executable_dir.join("nsfw-lib").join("src");
    if bundled.join("nsfw_scanner").is_dir() {
        return Ok(bundled);
    }

    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("nsfw-lib")
        .join("src");
    if development.join("nsfw_scanner").is_dir() {
        return Ok(development);
    }
    Err(format!(
        "Scanner library not found beside the GUI or in the development checkout: {}",
        bundled.display()
    ))
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct AppFlags {
    pub skip_generation_threshold: Option<u8>,
    pub skip_action_map: String,
    pub fps: f64,
    pub every_frame: bool,
    pub max_frames: u32,
    #[serde(default = "default_true")]
    pub subtitles: bool,
    pub tiles: String,
    pub hwaccel: String,
    pub frames: String,
    pub decoder: String,
    pub model: String,
    pub face_crop: bool,
    pub debug: bool,
    pub submit_to_webservice: bool,
    pub webservice_url: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SampleInfo {
    pub filename: String,
    pub rating: Option<u8>,
    pub blurred_path: String,
    pub original_path: String,
}

#[derive(Debug, Deserialize)]
struct SampleMetadata {
    rating: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct VideoProject {
    #[serde(rename = "project_id")]
    pub id: String,
    pub original_filename: String,
    pub sha256: String,
    pub timestamp: u64,
    pub flags: AppFlags,
    pub samples: Vec<String>,
    pub timeline: Vec<TimelineEntry>,
    #[serde(default)]
    pub generated_skiplist_path: Option<String>,
    #[serde(default)]
    pub generated_skiplist_entries: usize,
    #[serde(default)]
    pub generated_skiplist_preview: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TimelineEntry {
    pub frame: String,
    pub rating: u8,
    pub action: String,
}

#[tauri::command]
pub async fn scan_video_cmd(
    app: tauri::AppHandle,
    path: String,
    flags_json: String,
) -> Result<String, String> {
    // Parse flags
    let flags: AppFlags = serde_json::from_str(&flags_json)
        .map_err(|e| format!("Failed to parse flags: {}", e))?;
    
    // Build scanner config from flags
    let backend = match flags.model.as_str() {
        "python" => ScannerBackend::Python,
        "rust-cli" => ScannerBackend::RustCli,
        "rust-cdylib" => ScannerBackend::RustCdyLib,
        "auto" | _ => ScannerBackend::default(),
    };
    
    let config = ScannerConfig {
        backend,
        model_dir: std::env::var("NSFW_MODEL_DIR").ok(),
        python_path: std::env::var("PYTHON_PATH").ok(),
        rust_cli_path: std::env::var("NSFW_RUST_CLI_PATH").ok(),
        lib_path: std::env::var("NSFW_LIB_PATH").ok(),
        auto_download_models: true,
        model_base_url: std::env::var("NSFW_MODEL_BASE_URL").ok(),
    };
    
    let scanner = Scanner::new(config);
    let options = ScanOptions {
        fps: flags.fps,
        every_frame: flags.every_frame,
        max_frames: flags.max_frames,
        details: true,
        debug: flags.debug,
        subtitles: flags.subtitles,
        tiles: flags.tiles,
        hwaccel: flags.hwaccel,
        frames: flags.frames,
        decoder: flags.decoder,
        face_crop: flags.face_crop,
        save_samples: true,
        include_frame_ratings: true,
        ..Default::default()
    };
    
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
    let app_handle = app.clone();
    
    // Forward progress events
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = app_handle.emit("scan-progress", progress);
        }
    });
    
    let path_clone = path.clone();
    tokio::task::spawn_blocking(move || {
        scanner.scan_video(&std::path::PathBuf::from(path_clone), options, Some(progress_tx))
    })
    .await
    .map_err(|e| format!("Video scan task failed: {}", e))?
    .map(|result| serde_json::to_string(&result).map_err(|e| format!("Failed to encode result: {}", e)))?
}

#[tauri::command]
pub fn import_project_cmd(json_path: String) -> Result<VideoProject, String> {
    let project_path = PathBuf::from(json_path);
    let content = fs::read_to_string(&project_path)
        .map_err(|e| format!("Failed to read project file: {}", e))?;
    let mut project: VideoProject = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse project JSON: {}", e))?;
    let project_dir = project_path
        .parent()
        .ok_or_else(|| "Project file has no parent directory".to_string())?;
    for sample_path in &mut project.samples {
        let path = PathBuf::from(&sample_path);
        if path.is_relative() {
            *sample_path = project_dir.join(path).to_string_lossy().into_owned();
        }
    }
    if let Some(skiplist_path) = &mut project.generated_skiplist_path {
        let path = PathBuf::from(skiplist_path.as_str());
        if path.is_relative() {
            *skiplist_path = project_dir.join(path).to_string_lossy().into_owned();
        }
    }
    Ok(project)
}

#[tauri::command]
pub fn export_project_cmd(project_json: String, output_path: String) -> Result<String, String> {
    fs::write(output_path.clone(), project_json)
        .map_err(|e| format!("Failed to write project file: {}", e))?;
    Ok(output_path)
}

#[tauri::command]
pub fn export_skiplist_cmd(source_path: String, output_path: String) -> Result<String, String> {
    let source = PathBuf::from(source_path);
    let output = PathBuf::from(output_path);
    if source.extension().and_then(|extension| extension.to_str()) != Some("skp") {
        return Err("Generated skip-list source must be a .skp file".to_string());
    }
    if !output
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("skp"))
        .unwrap_or(false)
    {
        return Err("Export destination must use the .skp extension".to_string());
    }
    if !source.is_file() {
        return Err(format!(
            "Generated skip list not found: {}",
            source.display()
        ));
    }
    let same_file = match (fs::canonicalize(&source), fs::canonicalize(&output)) {
        (Ok(source), Ok(output)) => source == output,
        _ => false,
    };
    if !same_file {
        fs::copy(&source, &output)
            .map_err(|error| format!("Failed to export skip list: {}", error))?;
    }
    Ok(output.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn set_flags_cmd(flags_json: String) -> Result<String, String> {
    let flags: AppFlags =
        serde_json::from_str(&flags_json).map_err(|e| format!("Failed to parse flags: {}", e))?;
    Ok(format!(
        "Flags updated: model={}, fps={}, decoder={}, submit_to_webservice={}",
        flags.model, flags.fps, flags.decoder, flags.submit_to_webservice
    ))
}

#[tauri::command]
pub fn show_warning_cmd(path: String) -> Result<String, String> {
    Ok(format!(
        "Warning: The file '{}' may contain explicit content. Proceed with caution.",
        path
    ))
}

#[tauri::command]
pub async fn open_dialog_select(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter(
            "Video files",
            &["mp4", "mkv", "avi", "mov", "webm", "wmv", "m4v"],
        )
        .pick_file(move |file_path| {
            let _ = sender.send(file_path);
        });

    let Some(file_path) = receiver
        .await
        .map_err(|error| format!("Failed to receive selected media file: {}", error))?
    else {
        return Ok(None);
    };
    let path = file_path
        .into_path()
        .map_err(|error| format!("Failed to resolve selected media file: {}", error))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn list_samples_cmd(media_path: String) -> Result<Vec<SampleInfo>, String> {
    let mut samples: Vec<SampleInfo> = Vec::new();
    let media = PathBuf::from(media_path);
    let base_dir = if media.is_dir() {
        media
    } else {
        media
            .parent()
            .ok_or_else(|| "Media path has no parent directory".to_string())?
            .to_path_buf()
    };
    let samples_dir = base_dir.join("SAMPLES");
    if !samples_dir.exists() {
        return Ok(samples);
    }

    for entry in fs::read_dir(&samples_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let original = entry.path();
        let is_png = original
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("png"))
            .unwrap_or(false);
        if !original.is_file() || !is_png {
            continue;
        }
        if let Some(sample) = sample_info_from_original(&original, &base_dir)? {
            samples.push(sample);
        }
    }
    samples.sort_by(|left, right| left.filename.cmp(&right.filename));
    Ok(samples)
}

#[tauri::command]
pub fn list_project_samples_cmd(sample_paths: Vec<String>) -> Result<Vec<SampleInfo>, String> {
    let mut samples = Vec::new();
    for path in sample_paths {
        let original = PathBuf::from(path);
        let Some(base_dir) = original.parent().and_then(|dir| dir.parent()) else {
            continue;
        };
        if let Some(sample) = sample_info_from_original(&original, base_dir)? {
            samples.push(sample);
        }
    }
    samples.sort_by(|left, right| left.filename.cmp(&right.filename));
    Ok(samples)
}

fn sample_info_from_original(
    original: &std::path::Path,
    base_dir: &std::path::Path,
) -> Result<Option<SampleInfo>, String> {
    let is_png = original
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("png"))
        .unwrap_or(false);
    if !original.is_file() || !is_png {
        return Ok(None);
    }
    let stem = original
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Sample filename is not valid UTF-8".to_string())?;
    if stem.ends_with("-blurred") {
        return Ok(None);
    }
    let blurred = base_dir.join(format!("{}-blurred.png", stem));
    if !blurred.is_file() {
        return Ok(None);
    }
    let metadata_path = original.with_extension("json");
    let rating = if metadata_path.is_file() {
        let content = fs::read_to_string(&metadata_path).map_err(|error| {
            format!(
                "Failed to read sample metadata {}: {}",
                metadata_path.display(),
                error
            )
        })?;
        let metadata: SampleMetadata = serde_json::from_str(&content).map_err(|error| {
            format!(
                "Failed to parse sample metadata {}: {}",
                metadata_path.display(),
                error
            )
        })?;
        metadata.rating
    } else {
        None
    };
    Ok(Some(SampleInfo {
        filename: original
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        rating,
        blurred_path: blurred.to_string_lossy().to_string(),
        original_path: original.to_string_lossy().to_string(),
    }))
}

#[tauri::command]
pub async fn read_sample_cmd(path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        use base64::Engine;

        let sample = PathBuf::from(path);
        if sample
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| !ext.eq_ignore_ascii_case("png"))
            .unwrap_or(true)
        {
            return Err("Sample preview must be a PNG file".to_string());
        }
        let bytes = fs::read(&sample)
            .map_err(|e| format!("Failed to read sample {}: {}", sample.display(), e))?;
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(|e| format!("Sample read task failed: {}", e))?
}

#[tauri::command]
pub async fn scan_image_cmd(
    path: String,
    _flags_json: String,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let python = std::env::var("PYTHON_PATH").unwrap_or_else(|_| "python".to_string());
        let library_path = scanner_library_path()?;
        let script = "import json,sys; from nsfw_scanner import scan; print(json.dumps(scan(sys.argv[1], details=True, debug=True, use_badwords=False, use_known=False)))";
        let mut command = Command::new(python);
        hide_console_window(&mut command);
        let output = command
            .args(["-c", script])
            .arg(path)
            .env("PYTHONPATH", library_path)
            .output()
            .map_err(|e| format!("Failed to run python scanner: {}", e))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Scanner failed: {}", err));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(stdout.trim())
            .map_err(|e| format!("Failed to parse scanner output: {} (raw: {})", e, stdout))
    })
    .await
    .map_err(|e| format!("Image scan task failed: {}", e))?
}

#[tauri::command]
pub async fn rescan_sample_cmd(
    path: String,
    flags_json: String,
) -> Result<serde_json::Value, String> {
    let sample_path = PathBuf::from(&path);
    if !sample_path.is_file()
        || sample_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| !ext.eq_ignore_ascii_case("png"))
            .unwrap_or(true)
    {
        return Err("Only an existing PNG sample frame can be rescanned".to_string());
    }

    let result = scan_image_cmd(path, flags_json).await?;
    let metadata_path = sample_path.with_extension("json");
    let mut metadata = if metadata_path.is_file() {
        let content = fs::read_to_string(&metadata_path).map_err(|error| {
            format!(
                "Failed to read sample metadata {}: {}",
                metadata_path.display(),
                error
            )
        })?;
        serde_json::from_str::<serde_json::Value>(&content).map_err(|error| {
            format!(
                "Failed to parse sample metadata {}: {}",
                metadata_path.display(),
                error
            )
        })?
    } else {
        serde_json::json!({})
    };
    let Some(metadata_object) = metadata.as_object_mut() else {
        return Err(format!(
            "Sample metadata must be a JSON object: {}",
            metadata_path.display()
        ));
    };
    metadata_object.insert(
        "rating".to_string(),
        result
            .get("rating")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    );
    let content = serde_json::to_string_pretty(&metadata)
        .map_err(|error| format!("Failed to encode sample metadata: {}", error))?;
    fs::write(&metadata_path, content).map_err(|error| {
        format!(
            "Failed to update sample metadata {}: {}",
            metadata_path.display(),
            error
        )
    })?;
    Ok(result)
}
