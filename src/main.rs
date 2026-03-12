use std::collections::HashMap;
use std::sync::Arc;

use axum::{Extension, Router, routing::get};
use axum_server::tls_rustls::RustlsConfig;
use futures::lock::Mutex;

mod audio;
mod deepgram_response;
mod handlers;
mod state;
mod twilio_response;

#[tokio::main]
async fn main() {
    let proxy_url = std::env::var("PROXY_URL").unwrap_or_else(|_| "127.0.0.1:5000".to_string());

    let deepgram_url = std::env::var("DEEPGRAM_URL")
        .unwrap_or_else(|_| "wss://agent.deepgram.com/v1/agent/converse".to_string());

    let api_key =
        std::env::var("DEEPGRAM_API_KEY").expect("Using this server requires a Deepgram API Key.");

    let twilio_phone_number = std::env::var("TWILIO_PHONE_NUMBER")
        .expect("Using this server requires a Twilio phone number.");

    let cert_pem = std::env::var("CERT_PEM").ok();
    let key_pem = std::env::var("KEY_PEM").ok();

    let config = match (cert_pem, key_pem) {
        (Some(cert_pem), Some(key_pem)) => Some(
            RustlsConfig::from_pem_file(cert_pem, key_pem)
                .await
                .expect("Failed to make RustlsConfig from cert/key pem files."),
        ),
        (None, None) => None,
        _ => {
            panic!("Failed to start - invalid cert/key.")
        }
    };

    let state = Arc::new(state::State {
        deepgram_url,
        api_key,
        twilio_phone_number,
        games: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/twilio", get(handlers::twilio::twilio_handler))
        .route("/game", get(handlers::game::game_handler))
        .layer(Extension(state));

    let addr: std::net::SocketAddr = proxy_url.parse().unwrap();

    match config {
        Some(config) => {
            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service())
                .await
                .unwrap();
        }
        None => {
            axum_server::bind(addr)
                .serve(app.into_make_service())
                .await
                .unwrap();
        }
    }
}
