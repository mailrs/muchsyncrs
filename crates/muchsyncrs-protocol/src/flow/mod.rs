use crate::operation::MuchsyncProtocolMessage;

pub mod error;
pub mod ping;
pub mod status;

pub trait MuchsyncProtocolFlow<Operation> {
    type StartMessage: MuchsyncProtocolMessage + TryFrom<Operation> + Into<Operation> + 'static;
    type ResponseMessage: MuchsyncProtocolMessage + TryFrom<Operation> + Into<Operation> + 'static;
    type CompleteMessage: MuchsyncProtocolMessage + TryFrom<Operation> + Into<Operation> + 'static;
}
