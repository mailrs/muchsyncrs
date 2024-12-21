use crate::operation::Error;
use crate::operation::ErrorAcknowledgement;
use crate::operation::MuchsyncProtocolMessage;

pub struct ErrorOperation;

impl crate::flow::MuchsyncProtocolFlow<crate::operation::Operation> for ErrorOperation {
    type StartMessage = Error;
    type ResponseMessage = ErrorAcknowledgement;
    type CompleteMessage = CustomInfallible;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum CustomInfallible {}

impl From<CustomInfallible> for crate::operation::Operation {
    fn from(_: CustomInfallible) -> crate::operation::Operation {
        unreachable!()
    }
}

impl From<crate::operation::Operation> for CustomInfallible {
    fn from(_: crate::operation::Operation) -> Self {
        unreachable!()
    }
}

impl MuchsyncProtocolMessage for CustomInfallible {
    const MESSAGE_NAME: &'static str = panic!("Cannot instantiate CustomInfallible");
}
