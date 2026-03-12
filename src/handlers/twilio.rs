use std::sync::Arc;

use axum::{
    Extension,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose};
use futures::{
    channel::{mpsc, oneshot},
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::audio;
use crate::deepgram_response;
use crate::state::State;
use crate::twilio_response;

pub async fn twilio_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<State>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<State>) {
    let (this_sender, this_receiver) = socket.split();

    // prepare the connection request with the api key authentication
    let url: url::Url = state.deepgram_url.parse().expect("invalid Deepgram URL");
    let mut request = url.into_client_request().expect("cannot build WS request");
    let auth_value: http::HeaderValue = format!("Token {}", state.api_key)
        .parse()
        .expect("invalid auth header value");
    request.headers_mut().insert("Authorization", auth_value);

    // connect to deepgram
    let (deepgram_socket, _response) = connect_async(request)
        .await
        .expect("Failed to connect to Deepgram.");

    let (streamsid_tx, streamsid_rx) = oneshot::channel::<String>();

    let (deepgram_sender, deepgram_reader) = deepgram_socket.split();

    // channel for commessagesands to Deepgram writer
    let (deepgram_sender_tx, deepgram_sender_rx) = mpsc::channel::<tungstenite::Message>(32);

    // spawn writer that owns deepgram_sender
    tokio::spawn(async move {
        let mut deepgram_sender = deepgram_sender;
        let mut rx = deepgram_sender_rx;

        while let Ok(msg) = rx.recv().await {
            let _ = deepgram_sender.send(msg).await;
        }
    });

    tokio::spawn(handle_from_deepgram(
        Arc::clone(&state),
        deepgram_reader,
        this_sender,
        deepgram_sender_tx.clone(),
        streamsid_rx,
    ));
    tokio::spawn(handle_from_twilio(
        this_receiver,
        deepgram_sender_tx,
        streamsid_tx,
    ));
}

async fn handle_from_deepgram(
    state: Arc<State>,
    mut deepgram_receiver: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    mut twilio_sender: SplitSink<WebSocket, Message>,
    mut deepgram_sender_tx: mpsc::Sender<tungstenite::Message>,
    streamsid_rx: oneshot::Receiver<String>,
) {
    let streamsid = streamsid_rx
        .await
        .expect("Failed to receive streamsid from handle_from_twilio_ws.");

    while let Some(Ok(msg)) = deepgram_receiver.next().await {
        match msg {
            tungstenite::Message::Text(msg) => {
                dbg!(&msg);

                if let Ok(parsed) =
                    serde_json::from_str::<deepgram_response::ServerMessage>(msg.as_ref())
                {
                    match parsed {
                        deepgram_response::ServerMessage::FunctionCallRequest(
                            function_call_request,
                        ) => {
                            for function in function_call_request.functions {
                                if function.name == "strike" {
                                    dbg!("Strike ordered!");
                                }

                                let message = json!({
                                    "type": "FunctionCallResponse",
                                    "name": function.name,
                                    "content": "Success",
                                    "id": function.id,
                                });

                                let mut games = state.games.lock().await;
                                for game in games.values_mut() {
                                    let _ = game.send(Message::Text("STRIKE".into())).await;
                                }

                                let _ = deepgram_sender_tx
                                    .send(tungstenite::Message::Text(message.to_string().into()))
                                    .await;
                            }
                        }
                        deepgram_response::ServerMessage::UserStartedSpeaking => {
                            dbg!("SHOULD STOP PLAYBACK");
                            // Tell Twilio to stop any current playback
                            let clear = json!({
                                "event": "clear",
                                "streamSid": streamsid,
                            });

                            let _ = twilio_sender
                                .send(Message::Text(clear.to_string().into()))
                                .await;
                        }
                        _ => {}
                    }
                }

                let mut games = state.games.lock().await;
                for game in games.values_mut() {
                    let _ = game.send(Message::Text(msg.as_str().into())).await;
                }
            }
            tungstenite::Message::Binary(msg) => {
                // base64 encode the mulaw, wrap it in a Twilio media message, and send it to Twilio
                let base64_encoded_mulaw = general_purpose::STANDARD.encode(&msg);

                let sending_media =
                    twilio_response::SendingMedia::new(streamsid.clone(), base64_encoded_mulaw);

                let _ = twilio_sender
                    .send(Message::Text(
                        serde_json::to_string(&sending_media).unwrap().into(),
                    ))
                    .await;
            }
            _ => {}
        }
    }
}

async fn handle_from_twilio(
    mut this_receiver: SplitStream<WebSocket>,
    mut deepgram_sender_tx: mpsc::Sender<tungstenite::Message>,
    streamsid_tx: oneshot::Sender<String>,
) {
    let settings = json!({
        "type": "Settings",
        "audio": {
            "input": {
                "encoding": "mulaw",
                "sample_rate": 8000
            },
            "output": {
                "encoding": "mulaw",
                "sample_rate": 8000,
                "container": "none"
            }
        },
        "agent": {
            "listen": {
                "provider": {
                    "type": "deepgram",
                    "model": "flux-general-en"
                }
            },
            "think": {
                "provider": {
                    "type": "open_ai",
                    "model": "gpt-4o"
                },
                "prompt": "You are able to order Strikes on the map of the game Data Wars if the caller requests one. Reply in extremely short utterances.",
                "functions": [{
                    "name": "strike",
                    "description": "Order a Strike on the game Data Wars.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }]
            },
            "speak": {
                "provider": {
                    "type": "deepgram",
                    "model": "aura-asteria-en"
                }
            }
        }
    });

    let _ = deepgram_sender_tx
        .send(tungstenite::Message::Text(settings.to_string().into()))
        .await;

    let mut buffer_data = audio::BufferData {
        inbound_buffer: Vec::new(),
        inbound_last_timestamp: 0,
    };

    // wrap our oneshot in an Option because we will need it in a loop
    let mut streamsid_tx = Some(streamsid_tx);

    while let Some(Ok(msg)) = this_receiver.next().await {
        if let Message::Text(msg) = msg {
            let event: Result<twilio_response::Event, _> = serde_json::from_str(&msg);
            if let Ok(event) = event {
                match event.event_type {
                    twilio_response::EventType::Start(start) => {
                        // sending this streamsid on our oneshot will let `handle_from_deepgram` know the streamsid
                        if let Some(streamsid_tx) = streamsid_tx.take() {
                            streamsid_tx
                                .send(start.stream_sid.clone())
                                .expect("Failed to send streamsid to handle_to_game_rx.");
                        }
                    }
                    twilio_response::EventType::Media(media) => {
                        if let Some(audio) = audio::process_twilio_media(media, &mut buffer_data) {
                            // send the audio on to deepgram
                            if deepgram_sender_tx
                                .send(tungstenite::Message::Binary(audio.into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}
