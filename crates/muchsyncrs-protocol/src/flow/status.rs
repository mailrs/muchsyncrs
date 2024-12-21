use crate::operation::Status;
use crate::operation::StatusFin;
use crate::operation::StatusReply;

pub struct StatusOperation;

impl crate::flow::MuchsyncProtocolFlow<crate::operation::Operation> for StatusOperation {
    type StartMessage = Status;
    type ResponseMessage = StatusReply;
    type CompleteMessage = StatusFin;
}
