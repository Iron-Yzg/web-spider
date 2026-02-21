import { invoke } from '@tauri-apps/api/core'
import type { Website, AppConfig, ScrapeResult, PaginatedVideos, YtdlpTask, YtdlpConfig, ScraperInfo, LocalVideo } from '../types'

// ==================== 通用 API ====================

export async function getConfig(): Promise<AppConfig> {
  return await invoke<AppConfig>('get_config')
}

export async function updateConfig(config: Partial<AppConfig>): Promise<void> {
  const current = await getConfig()
  await invoke('update_config', { config: { ...current, ...config } })
}

export async function selectDirectory(): Promise<string | null> {
  return await invoke<string | null>('select_directory')
}

export async function checkFfmpeg(): Promise<{ ffmpeg: boolean; 'yt-dlp': boolean }> {
  return await invoke<{ ffmpeg: boolean; 'yt-dlp': boolean }>('check_ffmpeg')
}

// ==================== 视频管理 API ====================

export async function getVideos(page = 1, pageSize = 20): Promise<PaginatedVideos> {
  return await invoke<PaginatedVideos>('get_videos_paginated', { page, pageSize })
}

export async function searchVideos(query: string, page = 1, pageSize = 20): Promise<PaginatedVideos> {
  return await invoke<PaginatedVideos>('search_videos', { query, page, pageSize })
}

export async function getVideosByWebsite(websiteName: string, page = 1, pageSize = 20): Promise<PaginatedVideos> {
  return await invoke<PaginatedVideos>('get_videos_by_website', { websiteName, page, pageSize })
}

export async function deleteVideo(id: string): Promise<void> {
  await invoke('delete_video', { videoId: id })
}

export async function clearDownloadedVideos(): Promise<void> {
  await invoke('clear_downloaded')
}

export const clearDownloaded = clearDownloadedVideos

export async function downloadVideo(videoId: string): Promise<void> {
  await invoke('download_video', { videoId })
}

// ==================== 网站管理 API ====================

export async function getWebsites(): Promise<Website[]> {
  return await invoke<Website[]>('get_websites')
}

export async function getWebsiteByName(name: string): Promise<Website | null> {
  return await invoke<Website | null>('get_website_by_name', { name })
}

export async function saveWebsite(website: Omit<Website, 'id'> & { id?: string }): Promise<void> {
  await invoke('save_website', { website })
}

export async function deleteWebsite(id: string): Promise<void> {
  await invoke('delete_website', { websiteId: id })
}

export async function setDefaultWebsite(id: string): Promise<void> {
  await invoke('set_default_website', { websiteId: id })
}

export async function getScrapers(): Promise<ScraperInfo[]> {
  return await invoke<ScraperInfo[]>('get_scrapers')
}

// ==================== 爬虫 API ====================

export async function scrapeVideo(websiteId: string, url: string): Promise<ScrapeResult> {
  return await invoke<ScrapeResult>('scrape_video', { websiteId, url })
}

export async function batchDownload(videoIds: string[]): Promise<void> {
  await invoke('batch_download', { videoIds })
}

// ==================== yt-dlp API ====================

export async function getYtdlpConfig(): Promise<YtdlpConfig> {
  return await invoke<YtdlpConfig>('get_ytdlp_config')
}

export async function updateYtdlpConfig(config: Partial<YtdlpConfig>): Promise<void> {
  await invoke('update_ytdlp_config', { config })
}

export async function getYtdlpTasks(): Promise<YtdlpTask[]> {
  return await invoke<YtdlpTask[]>('get_ytdlp_tasks')
}

export async function getVideoInfo(url: string, quality: number = 1080): Promise<YtdlpTask> {
  return await invoke<YtdlpTask>('get_video_info', { url, quality })
}

export async function addYtdlpTasks(urls: string[], quality: number = 1080): Promise<void> {
  await invoke('add_ytdlp_tasks', { urls, quality })
}

export async function startYtdlpTask(taskId: string, outputPath: string): Promise<void> {
  await invoke('start_ytdlp_task', { taskId, outputPath })
}

export async function stopYtdlpTask(taskId: string): Promise<void> {
  await invoke('stop_ytdlp_task', { taskId })
}

export async function deleteYtdlpTask(taskId: string): Promise<void> {
  await invoke('delete_ytdlp_task', { taskId })
}

export async function cleanupYtdlpTasks(): Promise<void> {
  await invoke('cleanup_ytdlp_tasks')
}

export async function openPath(path: string): Promise<void> {
  await invoke('open_path', { path })
}

// ==================== 本地视频管理 API ====================

export async function getLocalVideos(): Promise<LocalVideo[]> {
  return await invoke<LocalVideo[]>('get_local_videos')
}

export async function addLocalVideo(video: LocalVideo): Promise<void> {
  await invoke('add_local_video', { video })
}

export async function deleteLocalVideo(id: string): Promise<void> {
  await invoke('delete_local_video_db', { id })
}

// ==================== DLNA 投屏 API ====================

export interface DlnaDevice {
  name: string
  udn: string
}

export type CastProtocol = 'auto' | 'sony' | 'dlna' | 'chromecast' | 'airplay'

export interface CastDevice {
  id: string
  name: string
  protocol: string
  available: boolean
  note?: string | null
}

export async function discoverDlnaDevices(timeoutSecs = 5): Promise<DlnaDevice[]> {
  return await invoke<DlnaDevice[]>('discover_dlna_devices', { timeoutSecs })
}

export async function getLocalIpAddress(): Promise<string> {
  return await invoke<string>('get_local_ip_address')
}

export async function startDlnaMediaServer(filePath: string, port = 8080): Promise<string> {
  return await invoke<string>('start_dlna_media_server', { filePath, port })
}

export async function stopDlnaMediaServer(): Promise<void> {
  await invoke('stop_dlna_media_server')
}

export async function stopDlnaPlayback(deviceName: string): Promise<void> {
  await invoke('stop_dlna_playback', { deviceName })
}

export async function castToDlnaDevice(deviceName: string, videoUrl: string, title: string): Promise<void> {
  await invoke('cast_to_dlna_device', { deviceName, videoUrl, title })
}

export async function discoverCastDevices(protocol: CastProtocol, timeoutSecs = 5): Promise<CastDevice[]> {
  return await invoke<CastDevice[]>('discover_cast_devices', { protocol, timeoutSecs })
}

export async function castMedia(protocol: CastProtocol, deviceId: string, videoUrl: string, title: string): Promise<void> {
  await invoke('cast_media', { protocol, deviceId, videoUrl, title })
}

export async function stopCastPlayback(protocol: CastProtocol, deviceId: string): Promise<void> {
  await invoke('stop_cast_playback', { protocol, deviceId })
}
