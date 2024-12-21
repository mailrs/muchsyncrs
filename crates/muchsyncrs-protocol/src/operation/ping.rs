use crate::operation::MuchsyncProtocolMessage;

impl MuchsyncProtocolMessage for Ping {
    const MESSAGE_NAME: &'static str = "ping";
}

#[allow(non_snake_case)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Ping {
    // empty
}

impl MuchsyncProtocolMessage for PingReply {
    const MESSAGE_NAME: &'static str = "pingRsp";
}

#[allow(non_snake_case)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PingReply {
    // empty
}

impl MuchsyncProtocolMessage for PingFin {
    const MESSAGE_NAME: &'static str = "pingCmp";
}

#[allow(non_snake_case)]
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PingFin {
    // empty
}
