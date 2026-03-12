use std::sync::Arc;

use axum::{
    Extension,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::SinkExt;
use futures::stream::StreamExt;

use crate::state::State;

pub async fn game_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<State>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<State>) {
    let id = uuid::Uuid::new_v4();

    let (mut game_sender, mut game_reader) = socket.split();

    // tell the game the phone number to call
    game_sender
        .send(Message::Text(state.twilio_phone_number.clone().into()))
        .await
        .expect("Failed to send the phone number to the game.");

    // insert a game ws (sender) handle for this game code, so that our Twilio handler can reference it
    let mut games = state.games.lock().await;
    games.insert(id, game_sender);
    drop(games);

    while let Some(Ok(msg)) = game_reader.next().await {
        dbg!(msg);
    }

    let mut games = state.games.lock().await;
    games.remove(&id);
    drop(games);
}
