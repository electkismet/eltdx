use std::net::ToSocketAddrs;
use std::time::Duration;

use clap::Parser;
use eltdx::{OperationOptions, TdxClient};

#[path = "http/gateway.rs"]
mod gateway;

#[derive(Parser)]
#[command(version, about = "Run the eltdx HTTP/WebSocket gateway")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8000)]
    port: u16,
    #[arg(long)]
    tdx_host: Option<String>,
    #[arg(long)]
    tdx_hosts: Option<String>,
    #[arg(long, default_value_t = 8.0)]
    timeout: f64,
    #[arg(long)]
    pool_size: Option<usize>,
    #[arg(long, default_value_t = 1)]
    server_count: usize,
    #[arg(long)]
    connections_per_server: Option<usize>,
    #[arg(long, default_value = "info", value_parser = ["critical", "error", "warning", "info", "debug", "trace"])]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    let listener = tokio::net::TcpListener::bind((args.host.as_str(), args.port)).await?;
    let show_startup = matches!(args.log_level.as_str(), "info" | "debug" | "trace");
    let client = tokio::task::spawn_blocking(move || {
        let timeout = Duration::try_from_secs_f64(args.timeout)?;
        let pool_size = match args.connections_per_server {
            Some(per_server) => args.server_count.checked_mul(per_server),
            None => Some(args.pool_size.unwrap_or(args.server_count)),
        }
        .filter(|size| *size > 0 && args.server_count > 0)
        .ok_or("invalid pool size or server count")?;
        let mut builder = TdxClient::builder()
            .request_timeout(timeout)
            .pool_size(pool_size);
        let candidates = args.tdx_host.as_deref().or(args.tdx_hosts.as_deref());
        if let Some(candidates) = candidates {
            let mut endpoints = Vec::new();
            for host in candidates
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
            {
                endpoints.extend(host.to_socket_addrs()?);
            }
            if !endpoints.is_empty() {
                builder = builder.endpoints(endpoints);
            }
        }
        let client = builder.build()?;
        client.connect(&OperationOptions::default())?;
        client.quotes().enable_push_frames()?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(client)
    })
    .await??;

    if show_startup {
        eprintln!("eltdx gateway listening on {}", listener.local_addr()?);
    }
    let (router, shutdown) = gateway::router(client.clone());
    let server = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            if let Ok(mut terminate) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! {
                    _ = terminate.recv() => {},
                    _ = tokio::signal::ctrl_c() => {},
                }
                shutdown.send_replace(true);
                return;
            }
            let _ = tokio::signal::ctrl_c().await;
            shutdown.send_replace(true);
        })
        .await;
    let closed = tokio::task::spawn_blocking(move || client.close()).await?;
    server?;
    closed?;
    Ok(())
}
