use std::{
    convert::Into,
    io::{self, Cursor, Read, Write},
    net::Shutdown,
    path::Path,
};

#[cfg(target_os = "windows")]
use uds_windows::{UnixListener, UnixStream};

use bincode::Options;

use crate::{
    editor_operation::{
        EditorOperation, EditorOperationDeserializeResult, EditorOperationDeserializer,
    },
    event::Key,
    event_manager::{EventRegistry, StreamId},
};

struct ReadBuf {
    buf: Vec<u8>,
    len: usize,
    position: usize,
}

impl ReadBuf {
    pub fn new() -> Self {
        let mut buf = Vec::with_capacity(2 * 1024);
        buf.resize(buf.capacity(), 0);
        Self {
            buf,
            len: 0,
            position: 0,
        }
    }

    pub fn slice(&self) -> &[u8] {
        &self.buf[self.position..self.len]
    }

    pub fn seek(&mut self, offset: usize) {
        self.position += offset;
        if self.position == self.len {
            self.len = 0;
            self.position = 0;
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.position = 0;
    }

    pub fn read_into<R>(&mut self, mut reader: R) -> io::Result<usize>
    where
        R: Read,
    {
        let mut total_bytes = 0;
        loop {
            match reader.read(&mut self.buf[self.len..]) {
                Ok(len) => {
                    total_bytes += len;
                    self.len += len;

                    if self.len < self.buf.len() {
                        break;
                    }

                    self.buf.resize(self.buf.len() * 2, 0);
                }
                Err(e) => match e.kind() {
                    io::ErrorKind::WouldBlock => break,
                    _ => return Err(e),
                },
            }
        }

        Ok(total_bytes)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TargetClient {
    All,
    Local,
    Remote(ConnectionWithClientHandle),
}

pub struct ConnectionWithClient {
    stream: UnixStream,
    read_buf: ReadBuf,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ConnectionWithClientHandle(usize);
impl ConnectionWithClientHandle {
    pub fn into_index(self) -> usize {
        self.0
    }
}
impl Into<ConnectionWithClientHandle> for StreamId {
    fn into(self) -> ConnectionWithClientHandle {
        ConnectionWithClientHandle(self.0)
    }
}
impl Into<StreamId> for ConnectionWithClientHandle {
    fn into(self) -> StreamId {
        StreamId(self.0)
    }
}

pub struct ConnectionWithClientCollection {
    listener: UnixListener,
    connections: Vec<Option<ConnectionWithClient>>,
    closed_connection_indexes: Vec<usize>,
}

impl ConnectionWithClientCollection {
    pub fn listen<P>(path: P) -> io::Result<Self>
    where
        P: AsRef<Path>,
    {
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;

        Ok(Self {
            listener,
            connections: Vec::new(),
            closed_connection_indexes: Vec::new(),
        })
    }

    pub fn register_listener(&self, event_registry: &EventRegistry) -> io::Result<()> {
        event_registry.register_listener(&self.listener)
    }

    pub fn listen_next_listener_event(&self, event_registry: &EventRegistry) -> io::Result<()> {
        event_registry.listen_next_listener_event(&self.listener)
    }

    pub fn accept_connection(
        &mut self,
        event_registry: &EventRegistry,
    ) -> io::Result<ConnectionWithClientHandle> {
        let (stream, _address) = self.listener.accept()?;
        stream.set_nonblocking(true)?;
        let connection = ConnectionWithClient {
            stream,
            read_buf: ReadBuf::new(),
        };

        for (i, slot) in self.connections.iter_mut().enumerate() {
            if slot.is_none() {
                let handle = ConnectionWithClientHandle(i);
                event_registry.register_stream(&connection.stream, handle.into())?;
                *slot = Some(connection);
                return Ok(handle);
            }
        }

        let handle = ConnectionWithClientHandle(self.connections.len());
        event_registry.register_stream(&connection.stream, handle.into())?;
        self.connections.push(Some(connection));
        Ok(handle)
    }

    pub fn listen_next_connection_event(
        &self,
        handle: ConnectionWithClientHandle,
        event_registry: &EventRegistry,
    ) -> io::Result<()> {
        if let Some(connection) = &self.connections[handle.0] {
            event_registry.listen_next_stream_event(&connection.stream, handle.into())?;
        }

        Ok(())
    }

    pub fn close_connection(&mut self, handle: ConnectionWithClientHandle) {
        if let Some(connection) = &self.connections[handle.0] {
            let _ = &connection.stream.shutdown(Shutdown::Both);
            self.closed_connection_indexes.push(handle.0);
        }
    }

    pub fn close_all_connections(&mut self) {
        for connection in self.connections.iter().flatten() {
            let _ = &connection.stream.shutdown(Shutdown::Both);
        }
    }

    pub fn unregister_closed_connections(
        &mut self,
        event_registry: &EventRegistry,
    ) -> io::Result<()> {
        for i in self.closed_connection_indexes.drain(..) {
            if let Some(connection) = self.connections[i].take() {
                event_registry.unregister_stream(&connection.stream)?;
            }
        }

        Ok(())
    }

    pub fn send_serialized_operations(&mut self, handle: ConnectionWithClientHandle, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let stream = match &mut self.connections[handle.0] {
            Some(connection) => &mut connection.stream,
            None => return,
        };

        if stream.write_all(bytes).is_err() {
            self.close_connection(handle);
        }
    }

    pub fn receive_key(&mut self, handle: ConnectionWithClientHandle) -> io::Result<Option<Key>> {
        match &mut self.connections[handle.0] {
            Some(connection) => deserialize(&mut connection.stream, &mut connection.read_buf),
            None => Ok(None),
        }
    }

    pub fn all_handles(&self) -> impl Iterator<Item = ConnectionWithClientHandle> {
        (0..self.connections.len()).map(|i| ConnectionWithClientHandle(i))
    }
}

pub struct ConnectionWithServer {
    stream: UnixStream,
    read_buf: ReadBuf,
}

impl ConnectionWithServer {
    pub fn connect<P>(path: P) -> io::Result<Self>
    where
        P: AsRef<Path>,
    {
        let stream = UnixStream::connect(path)?;
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            read_buf: ReadBuf::new(),
        })
    }

    pub fn close(&self) {
        let _ = &self.stream.shutdown(Shutdown::Both);
    }

    pub fn register_connection(&self, event_registry: &EventRegistry) -> io::Result<()> {
        event_registry.register_stream(&self.stream, StreamId(0))
    }

    pub fn listen_next_event(&self, event_registry: &EventRegistry) -> io::Result<()> {
        event_registry.listen_next_stream_event(&self.stream, StreamId(0))
    }

    pub fn send_key(&mut self, key: Key) -> io::Result<()> {
        match bincode_serializer().serialize_into(&mut self.stream, &key) {
            Ok(()) => Ok(()),
            Err(error) => Err(io::Error::new(io::ErrorKind::Other, error)),
        }
    }

    pub fn receive_operations<F>(&mut self, mut callback: F) -> io::Result<usize>
    where
        F: FnMut(EditorOperation<'_>),
    {
        self.read_buf.read_into(&mut self.stream)?;

        let mut operation_count = 0;
        let mut deserializer = EditorOperationDeserializer::from_slice(self.read_buf.slice());
        loop {
            match deserializer.deserialize_next() {
                EditorOperationDeserializeResult::Some(operation) => {
                    operation_count += 1;
                    callback(operation);
                }
                EditorOperationDeserializeResult::None => break,
                EditorOperationDeserializeResult::Error => {
                    return Err(io::Error::from(io::ErrorKind::Other))
                }
            }
        }

        self.read_buf.clear();
        Ok(operation_count)
    }
}

fn bincode_serializer() -> impl Options {
    bincode::options()
        .with_fixint_encoding()
        .allow_trailing_bytes()
}

fn deserialize<T>(mut reader: &mut UnixStream, buf: &mut ReadBuf) -> io::Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    loop {
        let slice = buf.slice();
        let deserializer = bincode_serializer().with_limit(slice.len() as _);
        let mut cursor = Cursor::new(slice);
        match deserializer.deserialize_from(&mut cursor) {
            Ok(value) => {
                let position = cursor.position() as _;
                buf.seek(position);
                break Ok(Some(value));
            }
            Err(error) => match error.as_ref() {
                bincode::ErrorKind::SizeLimit => (),
                _ => break Err(io::Error::new(io::ErrorKind::Other, error)),
            },
        }

        if buf.read_into(&mut reader)? == 0 {
            break Ok(None);
        }
    }
}
