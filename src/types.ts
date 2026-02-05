// 视频状态
export enum VideoStatus {
  Pending = 'Pending',
  Scraped = 'Scraped',
  Downloading = 'Downloading',
  Downloaded = 'Downloaded',
  Failed = 'Failed',
}

// 视频条目
export interface VideoItem {
  id: string
  name: string
  m3u8_url: string
  status: VideoStatus
  created_at: string
  downloaded_at?: string
  scrape_id: string
  website_name: string
  cover_url?: string
  view_count?: number
  favorite_count?: number
}

// 应用配置
export interface AppConfig {
  download_path: string
  local_storage: LocalStorageItem[]
  default_quality: string
}

// LocalStorage项
export interface LocalStorageItem {
  key: string
  value: string
}

// 爬取结果
export interface ScrapeResult {
  success: boolean
  name: string
  m3u8_url: string
  message: string
}

// 下载进度
export interface DownloadProgress {
  video_id: string
  progress: number
  status: string
  speed: string
  eta: string
}

// 分页结果
export interface PaginatedVideos {
  videos: VideoItem[]
  total: number
  page: number
  page_size: number
  has_more: boolean
}

// 网站配置
export interface Website {
  id: string
  name: string
  base_url: string
  local_storage: LocalStorageItem[]
  is_default: boolean
  spider: string
}

// 爬虫信息
export interface ScraperInfo {
  id: string
  name: string
}

// ==================== yt-dlp 下载相关类型 ====================

// yt-dlp 下载配置
export interface YtdlpConfig {
  // 视频质量：分辨率数值，如 0=最佳，480=480p，720=720p，1080=1080p，2160=4K
  // backend 的 build_format_string 函数会根据数值组装 yt-dlp 格式字符串
  quality: number
  format: string
  subtitles: boolean
  subtitle_langs: string
  thumbnail: boolean
  audio_only: boolean
  audio_format: string
  merge_video: boolean
  concurrent_downloads: number
  extra_options: string
}

// yt-dlp 任务状态
export enum YtdlpTaskStatus {
  Pending = 'Pending',
  Queued = 'Queued',
  Downloading = 'Downloading',
  Paused = 'Paused',
  Completed = 'Completed',
  Failed = 'Failed',
  Cancelled = 'Cancelled',
}

// yt-dlp 下载任务（简化版）
export interface YtdlpTask {
  id: string
  url: string
  title: string
  progress: number
  speed: string
  file_path?: string
  status: YtdlpTaskStatus
  message: string
  created_at: string
  completed_at?: string
}

// yt-dlp 下载结果
export interface YtdlpResult {
  success: boolean
  title: string
  file_path: string
  file_size: number
  message: string
}
