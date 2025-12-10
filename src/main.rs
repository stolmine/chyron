mod app;
mod cache;
mod config;
mod feeds;
mod ticker;
mod ui;

use anyhow::Result;
use clap::Parser;
use config::{CliArgs, Config};
use feeds::{FeedStatus, create_http_client, parse_feeds_file, validate_feed};

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();
    let config = Config::load(args)?;

    // Check if feeds file exists
    if !config.feeds_path.exists() {
        eprintln!("Error: Feeds file not found at {}", config.feeds_path.display());
        eprintln!();
        eprintln!("Create a feeds file with one URL per line:");
        eprintln!("  mkdir -p ~/.config/chyron");
        eprintln!("  echo 'https://example.com/rss' > ~/.config/chyron/urls");
        eprintln!();
        eprintln!("Or use an existing newsboat config at ~/.newsboat/urls");
        std::process::exit(1);
    }

    // Parse feed URLs
    let feed_urls = parse_feeds_file(&config.feeds_path).await?;

    if feed_urls.is_empty() {
        eprintln!("Error: No valid feed URLs found in {}", config.feeds_path.display());
        eprintln!("Add feed URLs (one per line) to the file.");
        std::process::exit(1);
    }

    println!("Found {} feed(s) in {}", feed_urls.len(), config.feeds_path.display());

    // Validate mode - check all feeds and exit
    if config.validate_only {
        return validate_feeds(&feed_urls).await;
    }

    // Run the main application
    let mut app = app::App::new(config).await?;
    app.run().await
}

async fn validate_feeds(urls: &[String]) -> Result<()> {
    println!();
    println!("Validating {} feed(s)...", urls.len());
    println!();

    let client = create_http_client()?;
    let mut success_count = 0;
    let mut error_count = 0;

    for url in urls {
        let result = validate_feed(&client, url).await;

        match result.status {
            FeedStatus::Ok { title, item_count } => {
                println!("  ✓ {} ({} items)", title, item_count);
                println!("    {}", url);
                success_count += 1;
            }
            FeedStatus::Error(err) => {
                println!("  ✗ Error: {}", err);
                println!("    {}", url);
                error_count += 1;
            }
        }
    }

    println!();
    println!("Summary: {} ok, {} failed", success_count, error_count);

    if error_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}
