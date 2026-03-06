use tauri::{Emitter, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::services::converter::{convert_video, generate_output_path, screenshot_video_frame, stop_convert_process, ConvertOptions, ConvertStatus, ConvertTask};

/// 开始格式转换
#[tauri::command]
pub async fn start_convert(
    app_handle: tauri::AppHandle,
    window: WebviewWindow,
    input_path: String,
    output_path: Option<String>,
    options: ConvertOptions,
) -> Result<ConvertTask, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let actual_output = output_path.unwrap_or_else(|| generate_output_path(&input_path, &options.format));

    // 发送初始任务状态
    let mut task = ConvertTask {
        id: task_id.clone(),
        input_path: input_path.clone(),
        output_path: actual_output.clone(),
        output_format: options.format.clone(),
        progress: 0,
        status: ConvertStatus::Converting,
        message: "开始转换...".to_string(),
    };
    let _ = window.emit("convert-progress", task.clone());

    // 启动转换
    let window_clone = window.clone();
    let task_id_clone = task_id.clone();
    let input_clone = input_path.clone();
    let output_clone = actual_output.clone();
    let format_clone = options.format.clone();
    let result = convert_video(
        &app_handle,
        &task_id,
        &input_path,
        &actual_output,
        &options,
        move |progress, msg| {
            let update = ConvertTask {
                id: task_id_clone.clone(),
                input_path: input_clone.clone(),
                output_path: output_clone.clone(),
                output_format: format_clone.clone(),
                progress,
                status: ConvertStatus::Converting,
                message: msg,
            };
            let _ = window_clone.emit("convert-progress", update);
        },
    )
    .await;

    match result {
        Ok(output) => {
            task.status = ConvertStatus::Completed;
            task.progress = 100;
            task.output_path = output;
            task.message = "转换完成".to_string();
            let _ = window.emit("convert-progress", task.clone());
            Ok(task)
        }
        Err(e) => {
            task.status = ConvertStatus::Failed;
            task.message = e.clone();
            let _ = window.emit("convert-progress", task.clone());
            Err(e)
        }
    }
}

/// 停止格式转换
#[tauri::command]
pub async fn stop_convert(task_id: String) -> Result<(), String> {
    if stop_convert_process(&task_id) {
        tracing::info!("[converter] 已停止转换任务: {}", task_id);
        Ok(())
    } else {
        Err(format!("未找到运行中的转换任务: {}", task_id))
    }
}

/// 视频截图
#[tauri::command]
pub async fn screenshot_video(
    app_handle: tauri::AppHandle,
    input_path: String,
    timestamp: f64,
    output_path: Option<String>,
) -> Result<String, String> {
    screenshot_video_frame(&app_handle, &input_path, timestamp, output_path).await
}

/// 选择要转换的文件
#[tauri::command]
pub async fn select_convert_input(window: WebviewWindow) -> Result<Option<String>, String> {
    let result: Option<tauri_plugin_dialog::FilePath> = window
        .dialog()
        .file()
        .set_title("选择要转换的文件")
        .add_filter("媒体文件", &[
            "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v",
            "mpg", "mpeg", "ts", "m4a", "mp3", "aac", "wav", "flac", "ogg",
        ])
        .blocking_pick_file();

    match result {
        Some(path) => Ok(Some(path.to_string())),
        None => Ok(None),
    }
}

/// 选择输出目录
#[tauri::command]
pub async fn select_convert_output(window: WebviewWindow) -> Result<Option<String>, String> {
    let result: Option<tauri_plugin_dialog::FilePath> = window
        .dialog()
        .file()
        .set_title("选择输出目录")
        .blocking_pick_folder();

    match result {
        Some(path) => Ok(Some(path.to_string())),
        None => Ok(None),
    }
}
