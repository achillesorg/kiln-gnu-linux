//! Unix-domain-socket transport for the Sway/i3 IPC protocol.
//!
//! This file is the only part of the Sway backend that needs the platform; it is
//! compiled when `#[cfg(unix)]` and never on the Windows authoring host, so it is
//! **not** runtime-verified here. The framing it relies on lives in
//! [`crate::sway`] and *is* unit-tested everywhere.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::sway::{self, HEADER_LEN};
use crate::BackendError;

/// An established, blocking connection to the sway IPC socket.
#[derive(Debug)]
pub(crate) struct SwayTransport {
    stream: UnixStream,
}

impl SwayTransport {
    /// Connect to the socket at `path` (the value of `$SWAYSOCK`).
    pub(crate) fn connect(path: &Path) -> Result<Self, BackendError> {
        let stream = UnixStream::connect(path)
            .map_err(|e| BackendError::Io(format!("connect {}: {e}", path.display())))?;
        // A request must never hang the shell's main loop: a stuck reply
        // fails after 2 s and the backend reconnects (see `SwayBackend`).
        let t = Some(std::time::Duration::from_secs(2));
        let _ = stream.set_read_timeout(t);
        let _ = stream.set_write_timeout(t);
        Ok(SwayTransport { stream })
    }

    /// Send one framed request and read the single framed reply payload.
    pub(crate) fn request(
        &mut self,
        msg_type: u32,
        payload: &[u8],
    ) -> Result<String, BackendError> {
        let frame = sway::encode(msg_type, payload);
        self.stream
            .write_all(&frame)
            .map_err(|e| BackendError::Io(e.to_string()))?;
        self.stream
            .flush()
            .map_err(|e| BackendError::Io(e.to_string()))?;

        let mut header = [0u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .map_err(|e| BackendError::Io(e.to_string()))?;
        if !sway::has_magic(&header) {
            return Err(BackendError::Protocol(
                "reply is not an i3-ipc frame".into(),
            ));
        }
        let (_msg_type, len) = sway::decode_header(&header);
        let mut body = vec![0u8; len as usize];
        self.stream
            .read_exact(&mut body)
            .map_err(|e| BackendError::Io(e.to_string()))?;
        String::from_utf8(body).map_err(|e| BackendError::Protocol(format!("non-UTF-8 reply: {e}")))
    }
}

/// The non-blocking `SUBSCRIBE` stream (window + workspace events).
pub(crate) struct SwayEvents {
    stream: UnixStream,
    frames: sway::FrameBuffer,
}

impl SwayEvents {
    /// Open a second connection, subscribe, read the ack, go non-blocking.
    pub(crate) fn connect(path: &Path) -> Result<Self, BackendError> {
        let mut t = SwayTransport::connect(path)?;
        let ack = t.request(sway::MSG_SUBSCRIBE, br#"["window","workspace"]"#)?;
        if !ack.contains("true") {
            return Err(BackendError::Protocol(format!("subscribe refused: {ack}")));
        }
        t.stream
            .set_nonblocking(true)
            .map_err(|e| BackendError::Io(e.to_string()))?;
        Ok(SwayEvents {
            stream: t.stream,
            frames: sway::FrameBuffer::default(),
        })
    }
}

impl crate::EventSource for SwayEvents {
    fn raw_fd(&self) -> i32 {
        use std::os::unix::io::AsRawFd;
        self.stream.as_raw_fd()
    }

    fn read_events(&mut self) -> Result<Vec<crate::BackendEvent>, BackendError> {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => return Err(BackendError::NotConnected("event stream closed".into())),
                Ok(n) => self.frames.push(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(BackendError::Io(e.to_string())),
            }
        }
        Ok(self
            .frames
            .frames()
            .into_iter()
            .map(|(t, p)| sway::parse_event(t, &p))
            .collect())
    }
}
