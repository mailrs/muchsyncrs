use std::pin::Pin;

use futures::Sink;
use futures::Stream;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

use crate::error::Error;
use crate::operation::Payload;

pub(crate) struct Transport {
    pub sink: Pin<Box<dyn Sink<Payload, Error = Error> + Send>>,
    pub stream: Pin<Box<dyn Stream<Item = Result<Payload, Error>> + Send>>,
}

pub type Reader<R> = tokio::io::BufReader<R>;
pub type Writer<W> = tokio::io::BufWriter<W>;

impl Transport {
    /// Helper fn for reading from reader to a buffer with a futures::stream::try_unfold() call
    pub fn from_io(
        input: impl AsyncRead + std::marker::Unpin + Send + 'static,
        output: impl AsyncWrite + std::marker::Unpin + Send + 'static,
    ) -> Transport {
        let stream = Box::pin(futures::stream::try_unfold(
            tokio::io::BufReader::new(input),
            Transport::read_to_eol,
        ));

        let sink = Box::pin(futures::sink::unfold(
            tokio::io::BufWriter::new(output),
            Transport::write_to_buffer,
        ));

        Transport { sink, stream }
    }

    async fn read_to_eol<R>(mut reader: Reader<R>) -> Result<Option<(Payload, Reader<R>)>, Error>
    where
        R: AsyncRead + std::marker::Unpin + Send + 'static,
    {
        let mut message_buffer = String::new();

        let eof = reader
            .read_line(&mut message_buffer)
            .await
            .map_err(Error::ReadingLine)?;

        if eof == 0 {
            return Ok(None);
        }

        let payload = serde_json::from_str(&message_buffer).map_err(Error::Deserialization)?;

        Ok(Some((payload, reader)))
    }

    /// Helper fn for writing a payload with a futures::sink::unfold() call
    async fn write_to_buffer<W>(mut writer: Writer<W>, payload: Payload) -> Result<Writer<W>, Error>
    where
        W: AsyncWrite + std::marker::Unpin + Send + 'static,
    {
        tracing::trace!(?payload, "Serializing payload for sending");
        let payload_buffer = serde_json::to_string(&payload).map_err(Error::Serialization)?;

        writer.write_all(payload_buffer.as_bytes()).await?;

        tracing::trace!(?payload_buffer, "Writing payload finished");
        Ok(writer)
    }
}
