use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Request, header},
    middleware::{self, Next},
    response::Response,
};
use inquire::Select;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app_state;
mod asset_types;
mod routes;

use app_state::AppState;
use std::path::Path;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut mode = None;
    let mut port = 80u16;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                mode = Some(match args.next().as_deref() {
                    Some("regular") => "Regular Mode",
                    Some("reflection") => "Reflection Mode",
                    Some("grab") => "Asset Grab Mode",
                    _ => panic!("--mode expects regular, reflection, or grab"),
                })
            }
            "--port" => {
                port = args
                    .next()
                    .expect("--port requires a value")
                    .parse()
                    .expect("invalid port")
            }
            "--help" | "-h" => {
                println!("studio_offline_server [--mode regular|reflection|grab] [--port PORT]");
                return;
            }
            _ => panic!("unknown argument: {arg}"),
        }
    }
    let mode = mode.unwrap_or_else(|| {
        Select::new(
            "How do you want to start Studio-Offline?",
            vec!["Asset Grab Mode", "Regular Mode", "Reflection Mode"],
        )
        .prompt()
        .unwrap_or("No mode selected")
    });
    if mode == "No mode selected" {
        return;
    }

    if mode == "Asset Grab Mode" && !std::path::Path::new("cookie.txt").exists() {
        tracing::error!("cookie.txt not found in the root directory.");
        tracing::error!(
            "This file is REQUIRED for Asset Grab Mode due to recent changes in Roblox's assetdelivery APIs."
        );
        tracing::error!("Please create a cookie.txt containing your .ROBLOSECURITY cookie.");
        return;
    }

    if !Path::new("./static").exists() {
        tracing::error!(
            "Static directory (required) not found in directory. Please reinstall Studio-Offline."
        );
        return;
    }

    println!("Webserver is running on mode: {mode}");

    let app_state = Arc::new(AppState {
        mode: mode.to_string(),
    });

    let app = Router::new()
        .nest(
            "/v2/settings/application/PCStudioApp",
            routes::client_settings::routes(),
        )
        .nest("/oauth", routes::oauth::routes())
        .nest("/assets", routes::upload::routes())
        .merge(routes::assets::routes())
        .merge(routes::static_handlers::routes())
        .merge(routes::telemetry::routes())
        .merge(routes::universal_app_config::routes())
        .with_state(app_state)
        .route(
            "/__studio_offline_health",
            axum::routing::get(|| async { "studio-offline" }),
        )
        .layer(middleware::from_fn_with_state(port, rewrite_loopback_urls));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

// Static fixtures and asset redirects assume port 80 upstream. Rewrite only
// loopback URLs when running on an unprivileged port; Roblox issuers stay intact.
async fn rewrite_loopback_urls(
    State(port): State<u16>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    if port == 80 {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let rewrite = |value: &str| {
        value
            .replace("http://localhost/", &format!("http://localhost:{port}/"))
            .replace("http://127.0.0.1/", &format!("http://127.0.0.1:{port}/"))
    };
    if let Some(location) = parts
        .headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Ok(value) = rewrite(location).parse() {
            parts.headers.insert(header::LOCATION, value);
        }
    }
    let text_body = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("json") || v.starts_with("text/") || v.contains("xml"));
    if !text_body {
        return Response::from_parts(parts, body);
    }
    match to_bytes(body, 16 * 1024 * 1024).await {
        Ok(bytes) => {
            let text = rewrite(&String::from_utf8_lossy(&bytes));
            parts.headers.remove(header::CONTENT_LENGTH);
            Response::from_parts(parts, Body::from(text))
        }
        Err(_) => Response::builder()
            .status(500)
            .body(Body::from("response too large"))
            .unwrap(),
    }
}
