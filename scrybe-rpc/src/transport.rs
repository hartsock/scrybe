// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Platform transport for the live-editor JSON-RPC endpoint.
//!
//! Unix retains its filesystem-domain socket. Windows uses a byte-mode named
//! pipe. Both carry the same newline-delimited JSON-RPC frames.

use std::io;
use std::path::Path;

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

#[cfg(windows)]
pub use interprocess::local_socket::{Listener, Stream};

#[cfg(windows)]
use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};

/// Bind the platform-local endpoint at `path`.
#[cfg(unix)]
pub fn listen_at(path: &Path) -> io::Result<Listener> {
    Listener::bind(path)
}

/// Bind a byte-mode Windows named pipe at `path`.
#[cfg(windows)]
pub fn listen_at(path: &Path) -> io::Result<Listener> {
    let name = path.to_fs_name::<GenericFilePath>()?;
    ListenerOptions::new().name(name).create_sync()
}

/// Accept one client connection.
#[cfg(unix)]
pub fn accept(listener: &Listener) -> io::Result<Stream> {
    listener.accept().map(|(stream, _address)| stream)
}

/// Accept one named-pipe client connection.
#[cfg(windows)]
pub fn accept(listener: &Listener) -> io::Result<Stream> {
    listener.accept()
}
