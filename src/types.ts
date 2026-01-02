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
