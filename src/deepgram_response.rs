use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Welcome { request_id: uuid::Uuid },
    FunctionCallRequest(DanglingFunctionCallRequests),
    UserStartedSpeaking,
}

#[derive(Deserialize, Serialize, Debug, Default, PartialEq, Clone)]
pub struct DanglingFunctionCallRequests {
    pub functions: Vec<FunctionCallRequest>,
}

#[derive(Deserialize, Serialize, Debug, Default, PartialEq, Clone)]
pub struct FunctionCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub client_side: bool,
}
