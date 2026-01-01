use std::net::SocketAddr;
use std::path::PathBuf;
use clap::Parser;
use m3u8_web_scraper::AppState;

#[derive(Parser, Debug)]
#[command(name = "m3u8-web-scraper")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server port
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Download directory
    #[arg(short, long, default_value = "./downloads")]
    download_dir: PathBuf,

    /// Config directory
    #[arg(short, long, default_value = "./data")]
    config_dir: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    println!("M3U8 Web Scraper Server");
    println!("========================");
    println!("Port: {}", args.port);
    println!("Download dir: {:?}", args.download_dir);
    println!("Config dir: {:?}", args.config_dir);

    // Create required directories
    tokio::fs::create_dir_all(&args.download_dir).await.unwrap();
    tokio::fs::create_dir_all(&args.config_dir).await.unwrap();

    // Initialize app state
    let app_state = AppState::new(args.download_dir, args.config_dir);

    // Build router
    let app = m3u8_web_scraper::build_router(app_state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    println!("\nServer running at http://localhost:{}", args.port);
    println!("Open http://localhost:{}/ in your browser", args.port);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
