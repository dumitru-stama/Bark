//! Background I/O manager using threads and channels.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use crate::async_io::{IoRequest, IoResponse};

/// Manages background I/O operations on a dedicated thread.
///
/// Requests are sent via `send()` and responses are polled via `try_recv()`.
/// The background thread automatically terminates when the IoManager is dropped.
pub struct IoManager {
    tx: Sender<IoRequest>,
    rx: Receiver<IoResponse>,
}

impl IoManager {
    /// Create a new IoManager with a background worker thread.
    #[must_use]
    pub fn new() -> Self {
        let (req_tx, req_rx) = channel::<IoRequest>();
        let (res_tx, res_rx) = channel::<IoResponse>();

        thread::spawn(move || {
            while let Ok(request) = req_rx.recv() {
                handle_request(request, &res_tx);
            }
        });

        Self {
            tx: req_tx,
            rx: res_rx,
        }
    }

    /// Send a request to the background worker.
    pub fn send(&self, req: IoRequest) {
        // Ignore send errors - they only occur if the receiver is dropped,
        // which means the worker thread has exited.
        let _ = self.tx.send(req);
    }

    /// Try to receive a response without blocking.
    /// Returns `None` if no response is available yet.
    #[must_use]
    pub fn try_recv(&self) -> Option<IoResponse> {
        self.rx.try_recv().ok()
    }
}

impl Default for IoManager {
    fn default() -> Self {
        Self::new()
    }
}

fn handle_request(req: IoRequest, tx: &Sender<IoResponse>) {
    match req {
        IoRequest::List(side, path, provider) => {
            let path_str = path.to_string_lossy().to_string();

            // Try to acquire lock on the provider
            let result = match provider.lock() {
                Ok(mut p) => p.list_directory(&path_str),
                Err(_) => {
                    // Mutex was poisoned (previous holder panicked)
                    let _ = tx.send(IoResponse::Error(
                        side,
                        "Provider lock poisoned".to_string(),
                    ));
                    return;
                }
            };

            match result {
                Ok(entries) => {
                    let _ = tx.send(IoResponse::Listed(side, path, entries));
                }
                Err(e) => {
                    let _ = tx.send(IoResponse::Error(side, e.to_string()));
                }
            }
        }
    }
}
