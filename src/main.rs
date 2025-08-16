use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use clap::{Parser, Subcommand};
// Load environment variables from .env (e.g., GITHUB_TOKEN)
use dotenvy::dotenv;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use url::Url;

#[derive(Parser)]
#[command(name = "github-dl")]
#[command(about = "Download GitHub folders", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download a GitHub folder
    Download {
        /// GitHub folder URL (e.g., https://github.com/owner/repo/tree/ref/path)
        link: String,
        /// Output directory to save the folder
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Refresh all downloaded folders in the base directory
    Refresh {
        /// Base directory to search for downloaded folders [default: current directory]
        #[arg(short, long, default_value = ".")]
        base_dir: PathBuf,
    },
}

#[derive(Clone, Serialize, Deserialize)]
struct Metadata {
    owner: String,
    repo: String,
    reference: String,
    path: String,
    url: String,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    // Load .env file into environment (e.g., GITHUB_TOKEN)
    dotenv().ok();
    let cli = Cli::parse();
    // Build HTTP client and wrap in Arc for thread-safe sharing
    let client = Arc::new(build_client()?);

    match cli.command {
        Commands::Download { link, output } => {
            let (parsed, _) = parse_github_link(&link)?;
            if output.exists() {
                if output.read_dir()?.next().is_some() {
                    return Err(format!("Output directory '{}' is not empty", output.display()).into());
                }
            }
            fs::create_dir_all(&output)?;
            let meta = Metadata {
                owner: parsed.owner,
                repo: parsed.repo,
                reference: parsed.reference,
                path: parsed.path,
                url: link.clone(),
            };
            let meta_path = output.join(".github-dl.json");
            let meta_json = serde_json::to_string_pretty(&meta)?;
            fs::write(&meta_path, meta_json)?;
            // Perform download
            download_dir(&client, &meta, &output)?;
            println!("Downloaded to {}", output.display());
        }
        Commands::Refresh { base_dir } => {
            let mut metas = Vec::new();
            find_metadata_files(&base_dir, &mut metas)?;
            if metas.is_empty() {
                println!("No downloaded folders found in {}", base_dir.display());
                return Ok(());
            }
            for meta_file in &metas {
                let meta_str = fs::read_to_string(&meta_file)?;
                let meta: Metadata = serde_json::from_str(&meta_str)?;
                let meta = Arc::new(meta);
                println!("Refreshing '{}'", meta.url);
                let listing_url = if meta.path.is_empty() {
                    format!("https://api.github.com/repos/{}/{}/contents?ref={}", meta.owner, meta.repo, meta.reference)
                } else {
                    format!("https://api.github.com/repos/{}/{}/contents/{}?ref={}", meta.owner, meta.repo, meta.path, meta.reference)
                };
                // Check if remote folder exists or we have permission to list it
                let resp = client.get(&listing_url).send()?;
                let status = resp.status();
                if status.as_u16() == 404 {
                    eprintln!("Remote folder {} does not exist, skipping", meta.url);
                    continue;
                } else if status.as_u16() == 403 {
                    return Err("Failed to list directory: HTTP 403 Forbidden. Are you hitting the GitHub API rate limit? Try setting the GITHUB_TOKEN environment variable.".into());
                } else if !status.is_success() {
                    return Err(format!("Failed to list directory: HTTP {}", status).into());
                }
                let local_dir = meta_file.parent().unwrap().to_path_buf();
                for entry in fs::read_dir(&local_dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.file_name() == Some(std::ffi::OsStr::new(".github-dl.json")) {
                        continue;
                    }
                    if path.is_dir() {
                        fs::remove_dir_all(&path)?;
                    } else {
                        fs::remove_file(&path)?;
                    }
                }
                // Refresh contents
                download_dir(&client, &meta, &local_dir)?;
                println!("Refreshed '{}'", meta.url);
            }
        }
    }

    Ok(())
}

struct ParsedLink {
    owner: String,
    repo: String,
    reference: String,
    path: String,
}

fn parse_github_link(link: &str) -> Result<(ParsedLink, Url), Box<dyn Error>> {
    let url = Url::parse(link)?;
    let host = url.host_str().ok_or("Invalid URL: missing host")?;
    if host != "github.com" {
        return Err("URL is not a github.com link".into());
    }
    let segments: Vec<_> = url.path_segments().ok_or("Cannot parse URL path segments")?.collect();
    if segments.len() < 4 || segments[2] != "tree" {
        return Err("URL must be in the format https://github.com/owner/repo/tree/ref[/path]".into());
    }
    let owner = segments[0].to_string();
    let repo = segments[1].to_string();
    let reference = segments[3].to_string();
    let path = if segments.len() > 4 {
        segments[4..].join("/")
    } else {
        String::new()
    };
    Ok((ParsedLink { owner, repo, reference, path }, url))
}

fn build_client() -> Result<Client, Box<dyn Error>> {
    let mut builder = Client::builder().user_agent("github-dl");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let mut headers = reqwest::header::HeaderMap::new();
        let value = format!("token {}", token);
        headers.insert(AUTHORIZATION, value.parse()?);
        builder = builder.default_headers(headers);
    }
    Ok(builder.build()?)
}

fn download_dir(client: &Client, meta: &Metadata, local_path: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(local_path)?;
    let listing_url = if meta.path.is_empty() {
        format!("https://api.github.com/repos/{}/{}/contents?ref={}", meta.owner, meta.repo, meta.reference)
    } else {
        format!("https://api.github.com/repos/{}/{}/contents/{}?ref={}", meta.owner, meta.repo, meta.path, meta.reference)
    };
    // Request directory listing; handle possible rate limiting
    let resp = client.get(&listing_url).send()?;
    let status = resp.status();
    if status.as_u16() == 403 {
        return Err("Failed to list directory: HTTP 403 Forbidden. Are you hitting the GitHub API rate limit? Try setting the GITHUB_TOKEN environment variable.".into());
    } else if !status.is_success() {
        return Err(format!("Failed to list directory: HTTP {}", status).into());
    }
    let items: Vec<Content> = resp.json()?;
    for item in items {
        let name = &item.name;
        let local_file_path = local_path.join(name);
        match item.r#type.as_str() {
            "file" => {
                if let Some(dl_url) = item.download_url {
                    let resp_file = client.get(&dl_url).send()?;
                    if !resp_file.status().is_success() {
                        return Err(format!("Failed to download file {}: HTTP {}", dl_url, resp_file.status()).into());
                    }
                    let bytes = resp_file.bytes()?;
                    fs::write(&local_file_path, &bytes)?;
                }
            }
            "dir" => {
                fs::create_dir_all(&local_file_path)?;
                let sub_path = if meta.path.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", meta.path, name)
                };
                let sub_meta = Metadata {
                    owner: meta.owner.clone(),
                    repo: meta.repo.clone(),
                    reference: meta.reference.clone(),
                    path: sub_path,
                    url: meta.url.clone(),
                };
                download_dir(client, &sub_meta, &local_file_path)?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct Content {
    name: String,
    #[serde(rename = "type")]
    r#type: String,
    download_url: Option<String>,
}

fn find_metadata_files(dir: &Path, result: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            find_metadata_files(&path, result)?;
        } else if entry.file_type()?.is_file() {
            if path.file_name() == Some(std::ffi::OsStr::new(".github-dl.json")) {
                result.push(path);
            }
        }
    }
    Ok(())
}