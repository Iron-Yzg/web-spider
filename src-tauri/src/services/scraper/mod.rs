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

    /// 爬取单个视频
    fn scrape(
        &self,
        video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = ScrapeResult> + Send>>;

    /// 爬取所有视频（每个爬虫必须实现）
    fn scrape_all(
        &self,
        video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = Vec<ScrapeResult>> + Send + 'static>>;
}

/// 爬虫类型枚举
#[derive(Clone)]
pub enum AnyScraper {
    D1(D1Spider),
    D2(D2Spider),
    Srl(SrlSpider),
}

impl AnyScraper {
    pub fn id(&self) -> &'static str {
        match self {
            AnyScraper::D1(scraper) => scraper.id(),
            AnyScraper::D2(scraper) => scraper.id(),
            AnyScraper::Srl(scraper) => scraper.id(),
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
            AnyScraper::D2(scraper) => scraper.scrape(video_id, log_callback),
            AnyScraper::Srl(scraper) => scraper.scrape(video_id, log_callback),
        }
    }

    fn scrape_all(
        &self,
        video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = Vec<ScrapeResult>> + Send>>
    where
        Self: Sized,
    {
        match self {
            AnyScraper::D1(scraper) => scraper.scrape_all(video_id, log_callback),
            AnyScraper::D2(scraper) => scraper.scrape_all(video_id, log_callback),
            AnyScraper::Srl(scraper) => scraper.scrape_all(video_id, log_callback),
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
        ScraperInfo {
            id: "d2".to_string(),
            name: "D2 CloudFront List".to_string(),
        },
        ScraperInfo {
            id: "srl".to_string(),
            name: "SRL Wiki".to_string(),
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
            "d2" => AnyScraper::D2(D2Spider::new(website)),
            "srl" => AnyScraper::Srl(SrlSpider::new(website)),
            _ => panic!("未知的爬虫: {}", website.spider),
        }
    }
}

// D1 爬虫实现
mod d1_spider;
pub use d1_spider::D1Spider;

// D2 爬虫实现
mod d2_spider;
pub use d2_spider::D2Spider;

// SRL 爬虫实现
mod srl_spider;
pub use srl_spider::SrlSpider;
