use serde::Serialize;
use serde::Serializer;

use crate::operation::MuchsyncProtocolMessage;

impl MuchsyncProtocolMessage for Status {
    const MESSAGE_NAME: &'static str = "status";
}

#[allow(non_snake_case)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Status {
    // empty
}

impl MuchsyncProtocolMessage for StatusReply {
    const MESSAGE_NAME: &'static str = "statusRsp";
}

fn deserialize_rxtime<'a, D: serde::Deserializer<'a>>(
    deserializer: D,
) -> Result<time::OffsetDateTime, D::Error> {
    let ts = <u64 as serde::Deserialize>::deserialize(deserializer)?;

    time::OffsetDateTime::from_unix_timestamp_nanos(ts as i128)
        .map_err(<D::Error as serde::de::Error>::custom)
}

fn serialize_rxtime<S: Serializer>(
    datetime: &time::OffsetDateTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    (datetime.unix_timestamp_nanos() as u64).serialize(serializer)
}

#[allow(non_snake_case)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StatusReply {
    /// Status code, using POSIX error numbers, 0 for “ok”
    pub code: i32,
    /// Status message
    pub message: String,
    #[serde(
        serialize_with = "serialize_rxtime",
        deserialize_with = "deserialize_rxtime"
    )]
    /// Unix UTC system time, 64 bit, ns resolution
    pub time: time::OffsetDateTime,
}

impl MuchsyncProtocolMessage for StatusFin {
    const MESSAGE_NAME: &'static str = "statusCmp";
}

#[allow(non_snake_case)]
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StatusFin {
    // empty
}
