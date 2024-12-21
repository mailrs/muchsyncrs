use std::collections::HashMap;
use std::marker::PhantomData;

use futures::future::BoxFuture;
use futures::FutureExt;
use thiserror::Error;
use tower::util::BoxService;
use tower::BoxError;
use tower::MakeService;
use tower::Service;
use tower::ServiceBuilder;
use tower::ServiceExt;

use crate::operation::Error;
use crate::operation::ErrorAcknowledgement;
use crate::flow::MuchsyncProtocolFlow;
use crate::operation::MuchsyncProtocolMessage;
use crate::operation::Operation;
use crate::operation::ServerFlow;
use crate::operation::ServerFlowSet;

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("The client sent an unexpected message: '{}'. '{}' or '{}' was expected.", .received, .expected, Error::MESSAGE_NAME)]
    ClientSentUnexpectedMessage {
        received: &'static str,
        expected: &'static str,
    },

    #[error("Server errored")]
    ServerError {
        #[source]
        ctx: BoxError,
    },
}

impl OperationError {
    pub fn to_protocol_error(&self) -> Error {
        Error {
            message: String::from("TODO"),
        }
    }
}

pub struct Router<MsgCtx> {
    map: HashMap<&'static str, ServerOperationHandler<MsgCtx>>,
}

impl<MsgCtx> Router<MsgCtx>
where
    MsgCtx: 'static,
{
    pub fn builder() -> RouterBuilder<MsgCtx> {
        RouterBuilder::new()
    }

    pub async fn handle_operation<Ctx>(
        &mut self,
        op: Operation,
        ctx: Ctx,
    ) -> Result<
        impl std::future::Future<Output = Result<OperationStatus, OperationError>>,
        OperationError,
    >
    where
        Ctx: OperationContext<MessageContext = MsgCtx>,
    {
        let Some(maker) = self.map.get_mut(op.name()) else {
            panic!()
        };

        Ok(ServiceExt::<(Ctx, State)>::ready(maker)
            .await?
            .call((ctx, State::Start(op))))
    }
}

pub struct RouterBuilder<MsgCtx> {
    map: HashMap<&'static str, ServerOperationHandler<MsgCtx>>,
}

impl<MsgCtx> RouterBuilder<MsgCtx>
where
    MsgCtx: 'static,
{
    fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }

    pub fn with_server_flow<MF>(mut self, flow: impl ServerFlow<Operation, MF, MsgCtx>) -> Self
    where
        MF: MuchsyncProtocolFlow<Operation>,
    {
        let handler = ServerOperationHandler::new(flow);

        self.map.insert(
            <MF::StartMessage as MuchsyncProtocolMessage>::MESSAGE_NAME,
            handler,
        );

        self
    }

    pub fn build(self) -> Router<MsgCtx> {
        Router { map: self.map }
    }
}

#[derive(Debug)]
pub struct OperationStatus {
    pub client: Option<Result<(), OperationError>>,
    pub server: Option<Result<(), OperationError>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum State {
    Start(Operation),
    Response(Operation),
    ErrorResponse(Operation),
    Complete(Operation),
    ErrorComplete(Operation),

    Done,
}

pub struct ServerOperationHandler<MsgCtx> {
    start_handler:
        BoxService<MsgCtx, BoxService<Operation, Operation, OperationError>, OperationError>,
    complete_handler: BoxService<MsgCtx, BoxService<Operation, (), OperationError>, OperationError>,
    complete_message_name: &'static str,
    error_handler:
        BoxService<MsgCtx, BoxService<Operation, Operation, OperationError>, OperationError>,
}

impl<MsgCtx> ServerOperationHandler<MsgCtx> {
    pub fn new<MF, SF>(flow: SF) -> Self
    where
        MF: MuchsyncProtocolFlow<Operation>,
        SF: ServerFlow<Operation, MF, MsgCtx>,
        MsgCtx: 'static,
    {
        let ServerFlowSet {
            start_handler_maker,
            complete_handler_maker,
            error_handler_maker,
        } = flow.get_handlers();

        let start_handler = ServiceBuilder::new()
            .boxed()
            .map_response(|start_handler| {
                ServiceBuilder::new()
                    .boxed()
                    .map_response(from_protocol_message)
                    .layer_fn(ToMuchsyncProtocolMessage::<MF::StartMessage, _>::new)
                    .service(start_handler)
            })
            .service(MakerService::new(start_handler_maker));

        let complete_handler = ServiceBuilder::new()
            .boxed()
            .map_response(|complete_handler| {
                ServiceBuilder::new()
                    .boxed()
                    .layer_fn(ToMuchsyncProtocolMessage::<MF::CompleteMessage, _>::new)
                    .service(complete_handler)
            })
            .service(MakerService::new(complete_handler_maker));

        let error_handler = ServiceBuilder::new()
            .boxed()
            .map_response(|error_handler| {
                ServiceBuilder::new()
                    .boxed()
                    .map_response(from_protocol_message)
                    .layer_fn(ToMuchsyncProtocolMessage::<Error, _>::new)
                    .service(error_handler)
            })
            .service(MakerService::new(error_handler_maker));

        ServerOperationHandler {
            start_handler,
            complete_handler,
            complete_message_name: MF::CompleteMessage::MESSAGE_NAME,
            error_handler,
        }
    }
}

pub struct StateFuture<Ctx> {
    start_handler: Option<BoxService<Operation, Operation, OperationError>>,
    complete_handler: Option<BoxService<Operation, (), OperationError>>,
    complete_message_name: &'static str,
    error_handler: Option<BoxService<Operation, Operation, OperationError>>,
    ctx: Ctx,
    state: State,
}

impl<Ctx> StateFuture<Ctx>
where
    Ctx: OperationContext,
{
    fn drive_to_completion(
        mut self,
    ) -> BoxFuture<'static, Result<OperationStatus, OperationError>> {
        async move {
            let mut status = OperationStatus {
                client: None,
                server: None,
            };
            loop {
                let state_state = std::mem::discriminant(&self.state);
                match self.state {
                    State::Start(operation) => {
                        let result = self.start_handler.take().unwrap().oneshot(operation).await;

                        match result {
                            Ok(response) => self.state = State::Response(response),
                            Err(err @ OperationError::ClientSentUnexpectedMessage { .. }) => {
                                self.state = State::Done;
                                status.server = Some(Err(err));
                            }
                            Err(error) => {
                                self.state = State::ErrorResponse(error.to_protocol_error().into());
                                status.server = Some(Err(error));
                            }
                        }
                    }
                    State::Response(ref operation) => {
                        self.ctx.send_operation(operation).await.unwrap();
                        let op = self.ctx.wait_for_operation().await.unwrap();
                        if op.name() == self.complete_message_name {
                            let result = self.complete_handler.take().unwrap().oneshot(op).await;

                            if let Err(error) = result {
                                status.server = Some(Err(error));
                            }

                            status.client = Some(Ok(()));
                            self.state = State::Done;
                        } else if op.name() == Error::MESSAGE_NAME {
                            let result = self.error_handler.take().unwrap().oneshot(op).await;
                            match result {
                                Ok(ack) => self.state = State::ErrorComplete(ack),
                                Err(error) => panic!("{:?}", error),
                            }
                        } else {
                            status.client =
                                Some(Err(OperationError::ClientSentUnexpectedMessage {
                                    received: op.name(),
                                    expected: self.complete_message_name,
                                }));
                            self.state = State::Done;
                        }
                    }
                    State::ErrorResponse(operation) => {
                        self.ctx.send_operation(&operation).await.unwrap();

                        let maybe_ack = self.ctx.wait_for_operation().await.unwrap();

                        if maybe_ack.name() == ErrorAcknowledgement::MESSAGE_NAME {
                            self.state = State::Done
                        } else {
                            panic!()
                        }
                    }
                    State::Complete(_operation) => {
                        unreachable!("Complete state not reachable in server impl")
                    }
                    State::ErrorComplete(operation) => {
                        self.ctx.send_operation(&operation).await.unwrap();
                        self.state = State::Done
                    }
                    State::Done => break,
                }

                let new_state_state = std::mem::discriminant(&self.state);

                debug_assert_ne!(
                    state_state, new_state_state,
                    "The state should never not advance between each loop"
                );

                self.ctx.update_state(&self.state).await.unwrap();
            }

            Ok(status)
        }
        .boxed()
    }
}

impl<Ctx, MsgCtx> Service<(Ctx, State)> for ServerOperationHandler<MsgCtx>
where
    Ctx: OperationContext<MessageContext = MsgCtx>,
{
    type Error = OperationError;
    type Response = OperationStatus;
    type Future = BoxFuture<'static, Result<OperationStatus, OperationError>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, (ctx, state): (Ctx, State)) -> Self::Future {
        let start_handler = self.start_handler.call(ctx.get_message_context());
        let complete_handler = self.complete_handler.call(ctx.get_message_context());
        let error_handler = self.error_handler.call(ctx.get_message_context());

        let complete_message_name = self.complete_message_name;
        async {
            let start_handler = start_handler.await?;
            let complete_handler = complete_handler.await?;
            let error_handler = error_handler.await?;

            let sfut = StateFuture {
                start_handler: Some(start_handler),
                complete_handler: Some(complete_handler),
                error_handler: Some(error_handler),
                complete_message_name,
                ctx,
                state,
            };

            sfut.drive_to_completion().await
        }
        .boxed()
    }
}

pub trait OperationContext: Send + Sync + 'static {
    type MessageContext: 'static;

    fn get_message_context(&self) -> Self::MessageContext;

    fn send_operation(
        &self,
        op: &Operation,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
    fn wait_for_operation(
        &self,
    ) -> impl std::future::Future<Output = Result<Operation, Error>> + Send;
    fn update_state<'s>(
        &'s self,
        state: &'s State,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send + 's;
}

pub trait ServiceCenterContext: Send + Sync + 'static {
    fn now_utc(&self) -> time::OffsetDateTime;
    fn system_uptime(&self) -> std::time::Duration;
}

#[derive(Debug)]
struct ToMuchsyncProtocolMessage<MM, S> {
    inner: S,
    _pd: PhantomData<fn() -> MM>,
}

impl<MM, S: Clone> Clone for ToMuchsyncProtocolMessage<MM, S> {
    fn clone(&self) -> Self {
        ToMuchsyncProtocolMessage {
            inner: self.inner.clone(),
            _pd: PhantomData,
        }
    }
}

impl<MM, S> Service<Operation> for ToMuchsyncProtocolMessage<MM, S>
where
    S: Service<MM, Error = OperationError, Future: Send + 'static>,
    MM: TryFrom<Operation> + MuchsyncProtocolMessage,
{
    type Error = OperationError;
    type Response = S::Response;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Operation) -> Self::Future {
        let req = to_protocol_message(req);

        let fut = req.map(|req| self.inner.call(req));
        async { fut?.await }.boxed()
    }
}

impl<MM, S> ToMuchsyncProtocolMessage<MM, S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            _pd: PhantomData,
        }
    }
}

fn to_protocol_message<MM>(op: Operation) -> Result<MM, OperationError>
where
    MM: MuchsyncProtocolMessage,
    MM: TryFrom<Operation>,
{
    let recv_op = op.name();
    MM::try_from(op).map_err(|_| OperationError::ClientSentUnexpectedMessage {
        received: recv_op,
        expected: MM::MESSAGE_NAME,
    })
}

fn from_protocol_message<MM>(mm: MM) -> Operation
where
    MM: MuchsyncProtocolMessage,
    MM: Into<Operation>,
{
    mm.into()
}

struct MakerService<M, Request> {
    make: M,
    _marker: PhantomData<Request>,
}

impl<M, Request> MakerService<M, Request> {
    fn new(make: M) -> Self {
        Self {
            make,
            _marker: PhantomData,
        }
    }
}

impl<M, S, Target, Request> Service<Target> for MakerService<M, Request>
where
    M: MakeService<Target, Request, Service = S>,
    S: Service<Request>,
{
    type Response = S;
    type Error = M::MakeError;
    type Future = <M as MakeService<Target, Request>>::Future;

    #[inline]
    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.make.poll_ready(cx)
    }

    #[inline]
    fn call(&mut self, target: Target) -> Self::Future {
        self.make.make_service(target)
    }
}

#[cfg(test)]
mod tests {
    use tower::service_fn;
    use tower::BoxError;
    use tower::ServiceExt;

    use super::OperationContext;
    use super::ServerOperationHandler;
    use super::ServiceCenterContext;
    use crate::error::ErrorAcknowledgement;
    use crate::flow::ping::PingOperation;
    use crate::operation::Operation;
    use crate::operation::ServerFlowSet;
    use crate::services::router::State;

    #[derive(Debug, Clone)]
    struct DummyCtx;

    #[allow(clippy::manual_async_fn)]
    impl OperationContext for DummyCtx {
        type MessageContext = DummyServiceCenterCtx;

        fn get_message_context(&self) -> Self::MessageContext {
            DummyServiceCenterCtx
        }

        async fn send_operation(&self, _op: &Operation) -> Result<(), crate::error::Error> {
            Ok(())
        }

        async fn wait_for_operation(&self) -> Result<Operation, crate::error::Error> {
            todo!()
        }

        fn update_state<'s>(
            &'s self,
            _state: &'s crate::services::router::State,
        ) -> impl std::future::Future<Output = Result<(), crate::error::Error>> + 's {
            async { Ok(()) }
        }
    }

    #[derive(Debug, Clone)]
    struct DummyServiceCenterCtx;

    #[allow(clippy::manual_async_fn)]
    impl ServiceCenterContext for DummyServiceCenterCtx {
        fn now_utc(&self) -> time::OffsetDateTime {
            time::OffsetDateTime::now_utc()
        }

        fn system_uptime(&self) -> std::time::Duration {
            std::time::Duration::from_millis(100)
        }
    }

    #[tokio::test]
    async fn check_invalid_operation_not_panicking() {
        let server_op = ServerOperationHandler::<DummyServiceCenterCtx>::new::<PingOperation, _>(
            ServerFlowSet::new(
                service_fn(|_| async {
                    Ok(service_fn(|_: crate::operation::ping::Ping| async {
                        Ok(crate::operation::ping::PingReply {})
                    }))
                }),
                service_fn(|_| async {
                    Ok(service_fn(
                        |_: crate::operation::ping::PingFin| async { Ok(()) },
                    ))
                }),
                service_fn(|_| async {
                    Ok(service_fn(|_: crate::error::Error| async {
                        Ok(ErrorAcknowledgement {})
                    }))
                }),
            ),
        );

        let resp = server_op
            .oneshot((
                DummyCtx,
                State::Start(Operation::PingReply(
                    crate::operation::ping::PingReply {},
                )),
            ))
            .await;

        insta::assert_debug_snapshot!(
            resp,
            @r#"
        Ok(
            OperationStatus {
                client: None,
                server: Some(
                    Err(
                        ClientSentUnexpectedMessage {
                            received: "pingRsp",
                            expected: "ping",
                        },
                    ),
                ),
            },
        )
        "#
        );
    }

    #[derive(Debug, Clone)]
    struct DummyErrorCtx;

    #[allow(clippy::manual_async_fn)]
    impl OperationContext for DummyErrorCtx {
        type MessageContext = DummyServiceCenterCtx;

        fn get_message_context(&self) -> Self::MessageContext {
            DummyServiceCenterCtx
        }

        async fn send_operation(&self, op: &Operation) -> Result<(), crate::error::Error> {
            assert!(matches!(op, Operation::Error(_)));
            Ok(())
        }

        async fn wait_for_operation(&self) -> Result<Operation, crate::error::Error> {
            Ok(Operation::ErrorAcknowledgement(ErrorAcknowledgement {}))
        }

        fn update_state<'s>(
            &'s self,
            _state: &'s crate::services::router::State,
        ) -> impl std::future::Future<Output = Result<(), crate::error::Error>> + 's {
            async { Ok(()) }
        }
    }

    #[tokio::test]
    async fn check_send_error_errors_properly() {
        let server_op = ServerOperationHandler::<DummyServiceCenterCtx>::new::<PingOperation, _>(
            ServerFlowSet::new(
                service_fn(|_| async {
                    Ok(service_fn(|_: crate::operation::ping::Ping| async {
                        Err(crate::services::router::OperationError::ServerError {
                            ctx: BoxError::from("foo"),
                        })
                    }))
                }),
                service_fn(|_| async {
                    Ok(service_fn(
                        |_: crate::operation::ping::PingFin| async { Ok(()) },
                    ))
                }),
                service_fn(|_| async {
                    Ok(service_fn(|_: crate::error::Error| async {
                        Ok(ErrorAcknowledgement {})
                    }))
                }),
            ),
        );

        let resp = server_op
            .oneshot((
                DummyErrorCtx,
                State::Start(Operation::Ping(crate::operation::ping::Ping {})),
            ))
            .await;

        insta::assert_debug_snapshot!(
            resp,
            @r#"
        Ok(
            OperationStatus {
                client: None,
                server: Some(
                    Err(
                        ServerError {
                            ctx: "foo",
                        },
                    ),
                ),
            },
        )
        "#
        );
    }
}
