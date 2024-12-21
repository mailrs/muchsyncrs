use tower::MakeService;
use tower_service::Service;

use crate::flow::MuchsyncProtocolFlow;
use crate::services::router::OperationError;

pub mod error;
pub mod ping;
pub mod status;

pub use self::error::*;
pub use self::ping::*;
pub use self::status::*;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Payload {
    pub op_id: u64,

    #[serde(flatten)]
    pub operation: Operation,
}

// helper types
//

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct Object(pub serde_json::Value);

#[derive(
    Clone, Debug, serde::Deserialize, serde::Serialize, derive_more::TryInto, derive_more::From,
)]
#[try_into(owned, ref)]
#[serde(tag = "command")]
pub enum Operation {
    #[serde(rename = "error")]
    Error(crate::operation::Error),

    #[serde(rename = "errorAck")]
    ErrorAcknowledgement(crate::operation::ErrorAcknowledgement),

    #[serde(rename = "ping")]
    Ping(crate::operation::Ping),

    #[serde(rename = "pingRsp")]
    PingReply(crate::operation::PingReply),

    #[serde(rename = "pingCmp")]
    PingFin(crate::operation::PingFin),

    #[serde(rename = "status")]
    Status(crate::operation::Status),

    #[serde(rename = "statusRsp")]
    StatusReply(crate::operation::StatusReply),

    #[serde(rename = "statusCmp")]
    StatusFin(crate::operation::StatusFin),
}

impl Operation {
    #[rustfmt::skip]
    pub fn name(&self) -> &'static str {
        match self {
            Operation::Error(_) => <crate::operation::Error as MuchsyncProtocolMessage>::MESSAGE_NAME,
            Operation::ErrorAcknowledgement(_) => <crate::operation::ErrorAcknowledgement as MuchsyncProtocolMessage>::MESSAGE_NAME,
            Operation::Ping(_) => <crate::operation::Ping as MuchsyncProtocolMessage>::MESSAGE_NAME,
            Operation::PingReply(_) => <crate::operation::PingReply as MuchsyncProtocolMessage>::MESSAGE_NAME,
            Operation::PingFin(_) => <crate::operation::PingFin as MuchsyncProtocolMessage>::MESSAGE_NAME,
            Operation::Status(_) => <crate::operation::Status as MuchsyncProtocolMessage>::MESSAGE_NAME,
            Operation::StatusReply(_) => <crate::operation::StatusReply as MuchsyncProtocolMessage>::MESSAGE_NAME,
            Operation::StatusFin(_) => <crate::operation::StatusFin as MuchsyncProtocolMessage>::MESSAGE_NAME,
        }
    }
}

/// Describe a operation
pub trait MuchsyncProtocolMessage:
    serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + Send
{
    const MESSAGE_NAME: &'static str;
}

pub struct ServerFlowSet<SH, CH, EH> {
    pub(crate) start_handler_maker: SH,
    pub(crate) complete_handler_maker: CH,
    pub(crate) error_handler_maker: EH,
}

impl<SH, CH, EH> ServerFlowSet<SH, CH, EH> {
    pub fn new(
        start_handler_maker: SH,
        complete_handler_maker: CH,
        error_handler_maker: EH,
    ) -> Self {
        ServerFlowSet {
            start_handler_maker,
            complete_handler_maker,
            error_handler_maker,
        }
    }
}

impl<Op, MF, Ctx, SH, CH, EH> ServerFlow<Op, MF, Ctx> for ServerFlowSet<SH, CH, EH>
where
    MF: MuchsyncProtocolFlow<Op>,
    SH: MakeService<
            Ctx,
            <MF as MuchsyncProtocolFlow<Op>>::StartMessage,
            Error = OperationError,
            Response = <MF as MuchsyncProtocolFlow<Op>>::ResponseMessage,
            MakeError = OperationError,
            Future: Send,
            Service: Service<<MF as MuchsyncProtocolFlow<Op>>::StartMessage, Future: Send>
                         + Send
                         + Sync
                         + 'static,
        > + Send
        + Sync
        + 'static,
    CH: MakeService<
            Ctx,
            <MF as MuchsyncProtocolFlow<Op>>::CompleteMessage,
            Error = OperationError,
            Response = (),
            MakeError = OperationError,
            Future: Send,
            Service: Service<<MF as MuchsyncProtocolFlow<Op>>::CompleteMessage, Future: Send>
                         + Send
                         + Sync
                         + 'static,
        > + Send
        + Sync
        + 'static,
    EH: MakeService<
            Ctx,
            Error,
            Error = OperationError,
            Response = ErrorAcknowledgement,
            MakeError = OperationError,
            Future: Send,
            Service: Service<Error, Future: Send> + Send + Sync + 'static,
        > + Send
        + Sync
        + 'static,
{
    type StartMessageHandlerMaker = SH;

    type CompleteMessageHandlerMaker = CH;

    type ErrorHandlerMaker = EH;

    fn get_handlers(
        self,
    ) -> ServerFlowSet<
        Self::StartMessageHandlerMaker,
        Self::CompleteMessageHandlerMaker,
        Self::ErrorHandlerMaker,
    > {
        self
    }
}

pub trait ServerFlow<Op, F, Ctx>
where
    F: MuchsyncProtocolFlow<Op>,
{
    type StartMessageHandlerMaker: MakeService<
            Ctx,
            <F as MuchsyncProtocolFlow<Op>>::StartMessage,
            Error = OperationError,
            Response = <F as MuchsyncProtocolFlow<Op>>::ResponseMessage,
            MakeError = OperationError,
            Future: Send,
            Service: Service<<F as MuchsyncProtocolFlow<Op>>::StartMessage, Future: Send>
                         + Send
                         + Sync
                         + 'static,
        > + Send
        + Sync
        + 'static;

    type CompleteMessageHandlerMaker: MakeService<
            Ctx,
            <F as MuchsyncProtocolFlow<Op>>::CompleteMessage,
            Error = OperationError,
            Response = (),
            MakeError = OperationError,
            Future: Send,
            Service: Service<<F as MuchsyncProtocolFlow<Op>>::CompleteMessage, Future: Send>
                         + Send
                         + Sync
                         + 'static,
        > + Send
        + Sync
        + 'static;

    type ErrorHandlerMaker: MakeService<
            Ctx,
            Error,
            Error = OperationError,
            Response = ErrorAcknowledgement,
            MakeError = OperationError,
            Future: Send,
            Service: Service<Error, Future: Send> + Send + Sync + 'static,
        > + Send
        + Sync
        + 'static;

    fn get_handlers(
        self,
    ) -> ServerFlowSet<
        Self::StartMessageHandlerMaker,
        Self::CompleteMessageHandlerMaker,
        Self::ErrorHandlerMaker,
    >;
}

pub trait ClientFlow<Op, F: MuchsyncProtocolFlow<Op>> {
    type ResponseHandler: Service<
            <F as MuchsyncProtocolFlow<Op>>::ResponseMessage,
            Error = Error,
            Response = <F as MuchsyncProtocolFlow<Op>>::CompleteMessage,
            Future: Send,
        > + Send
        + Sync
        + 'static;

    type ErrorHandler: Service<Error, Error = Error, Response = ErrorAcknowledgement, Future: Send>
        + Send
        + Sync
        + 'static;
}
