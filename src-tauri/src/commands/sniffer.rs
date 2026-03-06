use tauri::{Emitter, State, WebviewWindow};
use crate::db::{Database, SniffedMediaRecord};
use crate::services::sniffer::{SniffResult, sniff_page};

/// 嗅探页面中的媒体资源
#[tauri::command]
pub async fn sniff_media(
    window: WebviewWindow,
    db: State<'_, Database>,
    url: String,
    timeout_secs: Option<u64>,
) -> Result<SniffResult, String> {
    let timeout = timeout_secs.unwrap_or(5);

    // headless_chrome 是同步的，需要在阻塞线程中执行
    let result = tokio::task::spawn_blocking(move || {
        sniff_page(&url, timeout, |log| {
            let _ = window.emit("sniff-log", log);
        })
    })
    .await
    .map_err(|e| format!("嗅探任务执行失败: {}", e))?;

    if result.success {
        // 保存到数据库
        let now = chrono::Utc::now().to_rfc3339();
        for media in &result.media_list {
            let id = uuid::Uuid::new_v4().to_string();
            let _ = db.save_sniffed_media(
                &id,
                &result.page_url,
                &result.page_title,
                &media.url,
                &media.media_type,
                &media.file_ext,
                media.size,
                &media.source,
                &now,
            ).await;
        }
        Ok(result)
    } else {
        Err(result.message)
    }
}

/// 获取所有嗅探记录
#[tauri::command]
pub async fn get_sniffed_records(
    db: State<'_, Database>,
) -> Result<Vec<SniffedMediaRecord>, String> {
    db.get_all_sniffed_media()
        .await
        .map_err(|e| format!("获取嗅探记录失败: {}", e))
}

/// 删除嗅探记录
#[tauri::command]
pub async fn delete_sniffed_record(
    db: State<'_, Database>,
    id: String,
) -> Result<(), String> {
    db.delete_sniffed_media(&id)
        .await
        .map_err(|e| format!("删除嗅探记录失败: {}", e))
}

/// 清空所有嗅探记录
#[tauri::command]
pub async fn clear_sniffed_records(
    db: State<'_, Database>,
) -> Result<(), String> {
    db.clear_sniffed_media()
        .await
        .map_err(|e| format!("清空嗅探记录失败: {}", e))
}
