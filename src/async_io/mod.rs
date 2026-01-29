//! Asynchronous I/O operations for non-blocking directory listing.
//!
//! This module provides background I/O operations to prevent UI freezes
//! when listing remote directories (SCP) or large local directories.
//!
//! # Status
//! Currently implemented but not integrated into the main event loop.
//! Integration is planned for a future release.

// Allow dead code - this module is infrastructure for future async integration
#![allow(dead_code)]

pub mod manager;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use crate::fs::FileEntry;
use crate::state::Side;
use crate::providers::PanelProvider;

/// Request for an I/O operation
pub enum IoRequest {
    /// List directory contents: (target panel, path, provider)
    List(Side, PathBuf, Arc<Mutex<Box<dyn PanelProvider + Send>>>),
}

impl std::fmt::Debug for IoRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoRequest::List(side, path, _) => {
                f.debug_tuple("List")
                    .field(side)
                    .field(path)
                    .field(&"<provider>")
                    .finish()
            }
        }
    }
}

/// Response from an I/O operation
#[derive(Debug)]
pub enum IoResponse {
    /// Directory listing completed successfully
    Listed(Side, PathBuf, Vec<FileEntry>),
    /// I/O operation failed with error message
    Error(Side, String),
}