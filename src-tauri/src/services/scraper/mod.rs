use crate::models::{ScrapeResult, Website};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

/// 爬虫信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScraperInfo {
    pub id: String,
    pub name: String,
}

/// 爬虫 trait - 定义所有爬虫必须实现的方法
pub trait Scraper: Send + Sync {
    /// 获取爬虫标识名称
    fn id(&self) -> &'static str;

    /// 爬取视频
    fn scrape(
        &self,
        video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = ScrapeResult> + Send>>;
}

/// 爬虫类型枚举
#[derive(Clone)]
pub enum AnyScraper {
    D1(D1Spider),
}

impl AnyScraper {
    pub fn id(&self) -> &'static str {
        match self {
            AnyScraper::D1(scraper) => scraper.id(),
        }
    }
}

impl Scraper for AnyScraper {
    fn id(&self) -> &'static str {
        self.id()
    }

    fn scrape(
        &self,
        video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = ScrapeResult> + Send>> {
        match self {
            AnyScraper::D1(scraper) => scraper.scrape(video_id, log_callback),
        }
    }
}

/// 获取所有可用的爬虫列表
pub fn get_available_scrapers() -> Vec<ScraperInfo> {
    vec![
        ScraperInfo {
            id: "d1".to_string(),
            name: "D1 CloudFront".to_string(),
        },
    ]
}

/// 爬虫工厂
pub struct ScraperFactory;

impl ScraperFactory {
    /// 根据网站配置创建对应的爬虫
    pub fn create_scraper(website: &Website) -> AnyScraper {
        match website.spider.as_str() {
            "d1" => AnyScraper::D1(D1Spider::new(website)),
            _ => panic!("未知的爬虫: {}", website.spider),
        }
    }
}

// D1 爬虫实现
mod d1_spider;
pub use d1_spider::D1Spider;
