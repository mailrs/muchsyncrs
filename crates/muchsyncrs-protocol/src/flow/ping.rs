use crate::operation::Ping;
use crate::operation::PingFin;
use crate::operation::PingReply;

pub struct PingOperation;

impl crate::flow::MuchsyncProtocolFlow<crate::operation::Operation> for PingOperation {
    type StartMessage = Ping;
    type ResponseMessage = PingReply;
    type CompleteMessage = PingFin;
}
