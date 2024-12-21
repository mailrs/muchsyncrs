use std::collections::VecDeque;
use std::sync::Arc;

use futures::SinkExt;
use futures::StreamExt;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::error;
use tracing::trace;

pub use crate::flow::ping::PingOperation;
use crate::flow::status::StatusOperation;
use crate::flow::MuchsyncProtocolFlow;
use crate::operation::MuchsyncProtocolMessage;
use crate::operation::Operation;
use crate::operation::Payload;
use crate::operation::Ping;
use crate::operation::PingReply;

pub trait Receiver<Flow: MuchsyncProtocolFlow<Operation>> {
    fn on_start(
        &self,
        start: Flow::StartMessage,
    ) -> impl std::future::Future<Output = Flow::ResponseMessage>;
}

pub struct Session {
    id: uuid::Uuid,
    state: dashmap::DashMap<u64, WaitingState>,
    send_buffer: Mutex<SendBuffer>,
    buffer_pushed_notification: tokio::sync::Notify,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").finish_non_exhaustive()
    }
}

#[derive(Debug)]
enum WaitingState {
    WaitingForResponse(oneshot::Sender<Payload>),
    WaitingForCompletion,
}

struct SendBuffer {
    next_op_id: u32,
    buffer: VecDeque<Payload>,
}

impl SendBuffer {
    pub fn new() -> Self {
        Self {
            next_op_id: 0,
            buffer: VecDeque::new(),
        }
    }

    fn push_new_operation(&mut self, operation: Operation) -> u64 {
        let op_id = u64::from({
            let next_op_id = self.next_op_id;
            self.next_op_id = self
                .next_op_id
                .checked_add(1)
                .expect("Protocol error, ran out of OP ids");
            next_op_id
        });

        let payload = Payload { op_id, operation };

        self.buffer.push_back(payload);
        op_id
    }

    fn push_payload(&mut self, payload: Payload) {
        self.buffer.push_back(payload);
    }

    fn next(&self) -> Option<&Payload> {
        self.buffer.front()
    }

    fn pop_front(&mut self) {
        let _ = self.buffer.pop_front();
    }
}

impl Session {
    pub fn new(uuid: uuid::Uuid) -> Self {
        Self {
            id: uuid,
            state: dashmap::DashMap::new(),
            send_buffer: Mutex::new(SendBuffer::new()),
            buffer_pushed_notification: tokio::sync::Notify::new(),
        }
    }

    pub async fn handle_incoming_message(&self, message: Payload) -> Result<(), ()> {
        macro_rules! handle_flow {
            ($message:ident with $self:ident; receiving => [$($flow:ty),+ $(,)?], sending => [$($sflow:ty),+ $(,)?]) => {
                match $message.operation.name() {
                    $(
                    <<$flow as MuchsyncProtocolFlow<Operation>>::StartMessage as MuchsyncProtocolMessage>::MESSAGE_NAME => {
                        let Ok(operation): Result<<$flow as MuchsyncProtocolFlow<Operation>>::StartMessage, _> =
                            $message.operation.try_into()
                        else {
                            unreachable!()
                        };
                        tracing::debug!(op = <<$flow as MuchsyncProtocolFlow<Operation>>::StartMessage as MuchsyncProtocolMessage>::MESSAGE_NAME, "Handling operation");
                        let reply_message = <Self as Receiver<$flow>>::on_start($self, operation).await;
                        self.state
                            .insert($message.op_id, WaitingState::WaitingForCompletion);

                        self.send_payload(Payload {
                            op_id: $message.op_id,
                            operation: reply_message.into(),
                        })
                        .await?;
                    }
                    <<$flow as MuchsyncProtocolFlow<Operation>>::CompleteMessage as MuchsyncProtocolMessage>::MESSAGE_NAME => {
                        let Ok(_operation): Result<
                            <$flow as MuchsyncProtocolFlow<Operation>>::CompleteMessage,
                            _,
                        > = $message.operation.try_into() else {
                            unreachable!()
                        };
                        if $self.state.remove(&$message.op_id).is_none() {
                            panic!("Received a completion for something we did not reply to");
                        }
                        tracing::debug!(?$message.op_id, "Received completion");
                    }
                    )+
                    $(
                    <<$sflow as MuchsyncProtocolFlow<Operation>>::ResponseMessage as MuchsyncProtocolMessage>::MESSAGE_NAME => {
                        let Ok(operation): Result<
                            <$sflow as MuchsyncProtocolFlow<Operation>>::ResponseMessage,
                            _,
                        > = $message.operation.try_into() else {
                            unreachable!()
                        };
                        let Some((_, WaitingState::WaitingForResponse(resp_sender))) =
                            $self.state.remove(&$message.op_id)
                        else {
                            panic!("Received a response for something we do not expect");
                        };

                        if let Err(_) = resp_sender.send(Payload {
                            op_id: $message.op_id,
                            operation: operation.into(),
                        }) {
                            error!("Could not send response to waiting future");
                        }
                    }
                    )+
                    operation_name => {
                        tracing::debug!(?operation_name, "Received unhandled operation");
                    },
                }
            };
        }

        handle_flow!(message with self;
            receiving => [PingOperation],
            sending => [StatusOperation]
        );

        Ok(())
    }

    async fn send_payload(&self, payload: Payload) -> Result<(), ()> {
        self.send_buffer.lock().await.push_payload(payload);
        self.buffer_pushed_notification.notify_one();

        Ok(())
    }

    pub async fn send_message<Flow, Op, CallbackFut>(
        &self,
        op: Op,
        callback: impl FnOnce(<Flow as MuchsyncProtocolFlow<Operation>>::ResponseMessage) -> CallbackFut,
    ) -> Result<(), ()>
    where
        CallbackFut: std::future::Future<
            Output = <Flow as MuchsyncProtocolFlow<Operation>>::CompleteMessage,
        >,
        Op: MuchsyncProtocolMessage,
        Operation: From<Op>,
        Flow: MuchsyncProtocolFlow<Operation, StartMessage = Op>,
        Operation: From<<Flow as MuchsyncProtocolFlow<Operation>>::CompleteMessage>,
        <Flow as MuchsyncProtocolFlow<Operation>>::ResponseMessage: TryFrom<Operation>,
    {
        let (resp_sender, resp_receiver) = oneshot::channel();

        let op_id = self.send_buffer.lock().await.push_new_operation(op.into());
        self.buffer_pushed_notification.notify_one();
        self.state
            .insert(op_id, WaitingState::WaitingForResponse(resp_sender));

        let resp = resp_receiver.await.expect("to receive a proper respone");

        let resp_op: <Flow as MuchsyncProtocolFlow<Operation>>::ResponseMessage =
            match resp.operation.try_into() {
                Ok(op) => op,
                Err(_err) => panic!("Did not get correct operation"),
            };
        let resp_comp = callback(resp_op).await;

        self.send_buffer.lock().await.push_payload(Payload {
            op_id,
            operation: resp_comp.into(),
        });
        self.buffer_pushed_notification.notify_one();

        Ok(())
    }

    pub async fn send_error(
        &self,
        _error: crate::operation::Error,
    ) -> Result<crate::operation::ErrorAcknowledgement, ()> {
        todo!()
    }

    pub fn id(&self) -> uuid::Uuid {
        self.id
    }
}

impl Receiver<PingOperation> for Session {
    async fn on_start(&self, _: Ping) -> PingReply {
        PingReply {}
    }
}

#[cfg(test)]
#[allow(unused)]
fn test_compile() {
    use crate::operation::PingFin;

    let c = Session {
        state: dashmap::DashMap::new(),
        send_buffer: Mutex::new(SendBuffer::new()),
        buffer_pushed_notification: { unimplemented!() },
        id: unimplemented!(),
    };
    let _ = c.send_message::<PingOperation, _, _>(Ping {}, |resp| async { PingFin {} });
}

pub struct SendTask {
    connection: Arc<Session>,
    sink: std::pin::Pin<Box<dyn futures::Sink<Payload, Error = crate::error::Error> + Send>>,
}

impl SendTask {
    pub fn new(
        connection: Arc<Session>,
        sink: std::pin::Pin<Box<dyn futures::Sink<Payload, Error = crate::error::Error> + Send>>,
    ) -> Self {
        Self { connection, sink }
    }

    pub async fn start(mut self, cancellation_token: CancellationToken) {
        loop {
            let next = if let Some(lock) = cancellation_token
                .run_until_cancelled(self.connection.send_buffer.lock())
                .await
            {
                lock.next().cloned()
            } else {
                return;
            };
            tracing::trace!("Send buffer aquired");

            let Some(next) = next else {
                trace!("Did not get payload, waiting for notification");
                if cancellation_token
                    .run_until_cancelled(self.connection.buffer_pushed_notification.notified())
                    .await
                    .is_none()
                {
                    return;
                }

                continue;
            };
            trace!(?next, "Got payload");

            if let Some(res) = cancellation_token
                .run_until_cancelled(self.sink.send(next))
                .await
            {
                if let Err(error) = res {
                    error!(?error, "Could not send next item into sink");
                    break;
                }
            } else {
                tracing::debug!("Cancelled during sending to sink");
                return;
            }
            tracing::trace!("Sending to sink finished succesfully");

            if let Some(mut lock) = cancellation_token
                .run_until_cancelled(self.connection.send_buffer.lock())
                .await
            {
                lock.pop_front()
            }
            tracing::trace!("Send buffer front popped");
        }
    }
}

pub struct RecvTask {
    connection: Arc<Session>,
    stream:
        std::pin::Pin<Box<dyn futures::Stream<Item = Result<Payload, crate::error::Error>> + Send>>,
}

impl RecvTask {
    pub fn new(
        connection: Arc<Session>,
        stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<Payload, crate::error::Error>> + Send>,
        >,
    ) -> Self {
        Self { connection, stream }
    }

    pub async fn start(self, cancellation_token: CancellationToken) -> Result<(), ()> {
        let stream_fut = self
            .stream
            .map(|itm| async { itm })
            .buffered(3)
            .for_each_concurrent(3, |res| {
                let connection = self.connection.clone();
                async move {
                    let payload = match res {
                        Ok(payload) => payload,
                        Err(e) => {
                            tracing::error!(?e, "Failed to unwrap payload");
                            panic!("Failed to unwrap payload: {e:?}");
                        }
                    };
                    if let Err(()) = connection.handle_incoming_message(payload).await {
                        tracing::error!("Failed to handle payload");
                        panic!("Failed to handle payload");
                    }
                    tracing::trace!("Handled incoming message successfully");
                }
            });

        if cancellation_token
            .run_until_cancelled(stream_fut)
            .await
            .is_some()
        {
            debug!("Stream has closed");
        } else {
            debug!("Cancellation token was invoked, stopped stream");
        }

        Ok(())
    }
}
