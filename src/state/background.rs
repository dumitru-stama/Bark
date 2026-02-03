//! Background task handling for async operations

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};

use std::sync::Arc;

use crate::plugins::provider_api::PluginProviderAdapter;
use bark_plugin_api::{ProviderConfig, ProviderPlugin};
use crate::providers::{PanelProvider, ScpConnectionInfo, ScpProvider};
use crate::state::mode::FileOperation;
use crate::fs::utils::{copy_path_with_progress, move_path_with_progress, calculate_total_bytes};
use super::Side;

/// Progress update for file operations
#[derive(Clone, Debug)]
pub struct FileOpProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: String,
    pub files_done: usize,
    pub files_total: usize,
}

/// Result of a completed file operation
pub struct FileOpResult {
    pub count: usize,
    pub errors: Vec<String>,
    pub op_name: String,
}

/// Result of a background task
pub enum TaskResult {
    /// SCP connection succeeded
    ScpConnected {
        target: Side,
        provider: Box<ScpProvider>,
        initial_path: String,
        display_name: String,
        connection_key: Option<String>,
    },
    /// SCP connection failed
    ScpFailed {
        target: Side,
        error: String,
        /// If true, should prompt for password
        prompt_password: bool,
        /// Connection string for retry
        connection_string: Option<String>,
        display_name: String,
    },
    /// Plugin provider connection succeeded
    PluginConnected {
        target: Side,
        provider: Box<PluginProviderAdapter>,
        initial_path: String,
        display_name: String,
        /// If true, this is an extension-mode provider (e.g., archive plugin)
        /// and should use switch_to_extension_provider instead of set_provider
        is_extension_mode: bool,
        /// Source file path (for extension-mode providers, e.g., the archive file)
        source_path: Option<PathBuf>,
        /// Source file name (for extension-mode providers)
        source_name: Option<String>,
        connection_key: Option<String>,
    },
    /// Plugin provider connection failed
    PluginFailed {
        #[allow(dead_code)]
        target: Side,
        error: String,
        display_name: String,
        /// If true, the plugin requires a password (e.g., encrypted archive)
        password_required: bool,
        /// Source file path for retry (extension-mode plugins)
        source_path: Option<PathBuf>,
        /// Source file name for retry (extension-mode plugins)
        source_name: Option<String>,
    },
    /// File operation completed
    FileOpCompleted(FileOpResult),
    /// Remote file operation completed (providers need to be restored to panels)
    RemoteFileOpCompleted {
        result: FileOpResult,
        /// Provider(s) to return to panels
        src_provider: Option<Box<dyn PanelProvider>>,
        dest_provider: Option<Box<dyn PanelProvider>>,
        /// Which panel was the active (source) side
        active_side: Side,
    },
}

/// Metadata for a source file (pre-collected from panel entries before provider is taken)
#[allow(dead_code)]
pub struct SourceMeta {
    pub path: PathBuf,
    pub is_dir: bool,
    pub modified: Option<std::time::SystemTime>,
    pub permissions: u32,
    pub size: u64,
}

/// A background task with its communication channel
pub struct BackgroundTask {
    /// Receiver for task completion
    pub receiver: Receiver<TaskResult>,
    /// Progress receiver for file operations
    pub progress_rx: Option<Receiver<FileOpProgress>>,
    /// Thread handle (for cleanup)
    _handle: JoinHandle<()>,
}

impl BackgroundTask {
    /// Spawn a background SCP connection task
    pub fn connect_scp(
        conn_info: ScpConnectionInfo,
        target: Side,
        initial_path: String,
        display_name: String,
        connection_string: Option<String>,
        connection_key: Option<String>,
    ) -> Self {
        let (tx, rx) = channel::<TaskResult>();

        let handle = thread::spawn(move || {
            let mut provider = ScpProvider::new(conn_info);

            match provider.connect() {
                Ok(()) => {
                    let _ = tx.send(TaskResult::ScpConnected {
                        target,
                        provider: Box::new(provider),
                        initial_path,
                        display_name,
                        connection_key,
                    });
                }
                Err(e) => {
                    let _ = tx.send(TaskResult::ScpFailed {
                        target,
                        error: e.to_string(),
                        prompt_password: connection_string.is_some(),
                        connection_string,
                        display_name,
                    });
                }
            }
        });

        BackgroundTask {
            receiver: rx,
            progress_rx: None,
            _handle: handle,
        }
    }

    /// Check if the task has completed (non-blocking)
    pub fn try_recv(&self) -> Option<TaskResult> {
        self.receiver.try_recv().ok()
    }

    /// Spawn a background plugin provider connection task
    pub fn connect_plugin(
        plugin: Arc<dyn ProviderPlugin>,
        config: ProviderConfig,
        target: Side,
        display_name: String,
        connection_key: Option<String>,
    ) -> Self {
        let (tx, rx) = channel::<TaskResult>();
        let initial_path = config.get("path").unwrap_or("/").to_string();
        let plugin_info = plugin.info().clone();

        let handle = thread::spawn(move || {
            match plugin.connect(&config) {
                Ok(session) => {
                    let adapter = PluginProviderAdapter::new(session, &plugin_info);
                    let _ = tx.send(TaskResult::PluginConnected {
                        target,
                        provider: Box::new(adapter),
                        initial_path,
                        display_name,
                        is_extension_mode: false,
                        source_path: None,
                        source_name: None,
                        connection_key,
                    });
                }
                Err(e) => {
                    let _ = tx.send(TaskResult::PluginFailed {
                        target,
                        error: e.to_string(),
                        display_name,
                        password_required: false,
                        source_path: None,
                        source_name: None,
                    });
                }
            }
        });

        BackgroundTask {
            receiver: rx,
            progress_rx: None,
            _handle: handle,
        }
    }

    /// Spawn a background extension-mode plugin connection (e.g., archive plugin)
    pub fn connect_extension_plugin(
        plugin: Arc<dyn ProviderPlugin>,
        source_path: PathBuf,
        source_name: String,
        target: Side,
    ) -> Self {
        let (tx, rx) = channel::<TaskResult>();
        let plugin_info = plugin.info().clone();
        let display_name = source_name.clone();

        // Extension-mode plugins receive the local file path in config
        let mut config = ProviderConfig::new();
        config.set("path", source_path.to_string_lossy().to_string());
        config.name = source_name.clone();

        let sp = source_path.clone();
        let sn = source_name.clone();

        let handle = thread::spawn(move || {
            let sp_clone = sp.clone();
            let sn_clone = sn.clone();
            match plugin.connect(&config) {
                Ok(session) => {
                    let adapter = PluginProviderAdapter::new(session, &plugin_info);
                    let _ = tx.send(TaskResult::PluginConnected {
                        target,
                        provider: Box::new(adapter),
                        initial_path: "/".to_string(),
                        display_name,
                        is_extension_mode: true,
                        source_path: Some(sp),
                        source_name: Some(sn),
                        connection_key: None,
                    });
                }
                Err(e) => {
                    let is_pw = matches!(&e, bark_plugin_api::ProviderError::PasswordRequired(_));
                    let _ = tx.send(TaskResult::PluginFailed {
                        target,
                        error: e.to_string(),
                        display_name,
                        password_required: is_pw,
                        source_path: Some(sp_clone),
                        source_name: Some(sn_clone),
                    });
                }
            }
        });

        BackgroundTask {
            receiver: rx,
            progress_rx: None,
            _handle: handle,
        }
    }

    /// Spawn a background extension-mode plugin connection with password (e.g., encrypted archive)
    pub fn connect_extension_plugin_with_password(
        plugin: Arc<dyn ProviderPlugin>,
        source_path: PathBuf,
        source_name: String,
        target: Side,
        password: String,
    ) -> Self {
        let (tx, rx) = channel::<TaskResult>();
        let plugin_info = plugin.info().clone();
        let display_name = source_name.clone();

        let mut config = ProviderConfig::new();
        config.set("path", source_path.to_string_lossy().to_string());
        config.set("password", password);
        config.name = source_name.clone();

        let sp = source_path.clone();
        let sn = source_name.clone();

        let handle = thread::spawn(move || {
            let sp_clone = sp.clone();
            let sn_clone = sn.clone();
            match plugin.connect(&config) {
                Ok(session) => {
                    let adapter = PluginProviderAdapter::new(session, &plugin_info);
                    let _ = tx.send(TaskResult::PluginConnected {
                        target,
                        provider: Box::new(adapter),
                        initial_path: "/".to_string(),
                        display_name,
                        is_extension_mode: true,
                        source_path: Some(sp),
                        source_name: Some(sn),
                        connection_key: None,
                    });
                }
                Err(e) => {
                    let is_pw = matches!(&e, bark_plugin_api::ProviderError::PasswordRequired(_));
                    let _ = tx.send(TaskResult::PluginFailed {
                        target,
                        error: e.to_string(),
                        display_name,
                        password_required: is_pw,
                        source_path: Some(sp_clone),
                        source_name: Some(sn_clone),
                    });
                }
            }
        });

        BackgroundTask {
            receiver: rx,
            progress_rx: None,
            _handle: handle,
        }
    }

    /// Spawn a background file operation (local-to-local copy or move)
    pub fn file_operation(
        operation: FileOperation,
        sources: Vec<PathBuf>,
        dest: PathBuf,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        let (tx, rx) = channel::<TaskResult>();
        let (progress_tx, progress_rx) = channel::<FileOpProgress>();

        let bytes_total = calculate_total_bytes(&sources);
        let files_total = sources.len();

        let handle = thread::spawn(move || {
            let mut count = 0usize;
            let mut errors = Vec::new();
            let bytes_done = Arc::new(AtomicU64::new(0));
            // Single file to a non-directory destination = rename
            let is_rename = sources.len() == 1 && !dest.is_dir();

            for (i, src_path) in sources.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }

                let file_name = src_path.file_name().unwrap_or_default();
                let dest_file = if is_rename {
                    dest.clone()
                } else {
                    dest.join(file_name)
                };

                // Send progress update with current file
                let current_name = file_name.to_string_lossy().to_string();
                let _ = progress_tx.send(FileOpProgress {
                    bytes_done: bytes_done.load(Ordering::Relaxed),
                    bytes_total,
                    current_file: current_name.clone(),
                    files_done: i,
                    files_total,
                });

                let bd = bytes_done.clone();
                let ptx = progress_tx.clone();
                let cn = current_name.clone();
                let progress_cb = move |chunk: u64| {
                    bd.fetch_add(chunk, Ordering::Relaxed);
                    let _ = ptx.send(FileOpProgress {
                        bytes_done: bd.load(Ordering::Relaxed),
                        bytes_total,
                        current_file: cn.clone(),
                        files_done: i,
                        files_total,
                    });
                };

                let result = match &operation {
                    FileOperation::Copy => {
                        copy_path_with_progress(src_path, &dest_file, &cancel, &progress_cb)
                    }
                    FileOperation::Move => {
                        move_path_with_progress(src_path, &dest_file, &cancel, &progress_cb)
                    }
                    FileOperation::Delete => unreachable!(),
                };

                match result {
                    Ok(()) => count += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => break,
                    Err(e) => errors.push(format!("{}: {}", src_path.display(), e)),
                }
            }

            let op_name = match operation {
                FileOperation::Copy => "Copied",
                FileOperation::Move => "Moved",
                FileOperation::Delete => "Deleted",
            }.to_string();

            let _ = tx.send(TaskResult::FileOpCompleted(FileOpResult {
                count,
                errors,
                op_name,
            }));
        });

        BackgroundTask {
            receiver: rx,
            progress_rx: Some(progress_rx),
            _handle: handle,
        }
    }

    /// Spawn a background remote file operation (copy or move involving at least one remote provider).
    ///
    /// The caller must take providers out of the panels before calling this.
    /// They will be returned via `TaskResult::RemoteFileOpCompleted`.
    pub fn remote_file_operation(
        operation: FileOperation,
        source_metas: Vec<SourceMeta>,
        dest: PathBuf,
        mut src_provider: Option<Box<dyn PanelProvider>>,
        mut dest_provider: Option<Box<dyn PanelProvider>>,
        active_side: Side,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        let (tx, rx) = channel::<TaskResult>();
        let (progress_tx, progress_rx) = channel::<FileOpProgress>();

        let files_total = source_metas.len();
        let bytes_total: u64 = source_metas.iter().map(|m| m.size).sum();
        let is_rename = source_metas.len() == 1 && !dest.is_dir();
        let src_is_remote = src_provider.is_some();
        let dest_is_remote = dest_provider.is_some();

        let handle = thread::spawn(move || {
            let mut count = 0usize;
            let mut errors: Vec<String> = Vec::new();
            let bytes_done = Arc::new(AtomicU64::new(0));

            for (i, meta) in source_metas.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }

                let file_name = meta.path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let dest_file = if is_rename {
                    dest.clone()
                } else {
                    dest.join(&file_name)
                };

                // Send progress
                let _ = progress_tx.send(FileOpProgress {
                    bytes_done: bytes_done.load(Ordering::Relaxed),
                    bytes_total,
                    current_file: file_name.clone(),
                    files_done: i,
                    files_total,
                });

                let result: Result<(), String> = match &operation {
                    FileOperation::Copy => {
                        Self::remote_copy_one(
                            &meta.path, &dest_file, meta.modified, meta.permissions,
                            src_is_remote, dest_is_remote,
                            &mut src_provider, &mut dest_provider,
                        )
                    }
                    FileOperation::Move => {
                        // Move = copy + delete source
                        let copy_result = Self::remote_copy_one(
                            &meta.path, &dest_file, meta.modified, meta.permissions,
                            src_is_remote, dest_is_remote,
                            &mut src_provider, &mut dest_provider,
                        );
                        if copy_result.is_ok() {
                            // Delete source
                            if src_is_remote {
                                if let Some(ref mut prov) = src_provider {
                                    let path_str = meta.path.to_string_lossy().to_string();
                                    prov.delete(&path_str).map_err(|e| e.to_string())
                                } else {
                                    Ok(())
                                }
                            } else {
                                std::fs::remove_file(&meta.path).map_err(|e| e.to_string())
                            }
                        } else {
                            copy_result
                        }
                    }
                    FileOperation::Delete => unreachable!(),
                };

                // Update bytes done (approximate: add entire file size after completion)
                bytes_done.fetch_add(meta.size, Ordering::Relaxed);

                match result {
                    Ok(()) => count += 1,
                    Err(e) => errors.push(format!("{}: {}", meta.path.display(), e)),
                }
            }

            let op_name = match operation {
                FileOperation::Copy => "Copied",
                FileOperation::Move => "Moved",
                FileOperation::Delete => "Deleted",
            }.to_string();

            let _ = tx.send(TaskResult::RemoteFileOpCompleted {
                result: FileOpResult { count, errors, op_name },
                src_provider,
                dest_provider,
                active_side,
            });
        });

        BackgroundTask {
            receiver: rx,
            progress_rx: Some(progress_rx),
            _handle: handle,
        }
    }

    /// Helper: copy one file between providers.
    fn remote_copy_one(
        src_path: &PathBuf,
        dest_file: &PathBuf,
        modified: Option<std::time::SystemTime>,
        permissions: u32,
        src_is_remote: bool,
        dest_is_remote: bool,
        src_provider: &mut Option<Box<dyn PanelProvider>>,
        dest_provider: &mut Option<Box<dyn PanelProvider>>,
    ) -> Result<(), String> {
        let path_str = src_path.to_string_lossy().to_string();

        match (src_is_remote, dest_is_remote) {
            (true, false) => {
                // Remote to local: download
                let prov = src_provider.as_mut().unwrap();
                let data = prov.read_file(&path_str).map_err(|e| e.to_string())?;
                std::fs::write(dest_file, data).map_err(|e| e.to_string())?;
                apply_local_attributes(dest_file, modified, permissions);
                Ok(())
            }
            (false, true) => {
                // Local to remote: upload
                let data = std::fs::read(src_path).map_err(|e| e.to_string())?;
                let dest_str = dest_file.to_string_lossy().to_string();
                let prov = dest_provider.as_mut().unwrap();
                prov.write_file(&dest_str, &data).map_err(|e| e.to_string())?;
                let _ = prov.set_attributes(&dest_str, modified, permissions);
                Ok(())
            }
            (true, true) => {
                // Remote to remote: download then upload
                let data = {
                    let prov = src_provider.as_mut().unwrap();
                    prov.read_file(&path_str).map_err(|e| e.to_string())?
                };
                let dest_str = dest_file.to_string_lossy().to_string();
                let prov = dest_provider.as_mut().unwrap();
                prov.write_file(&dest_str, &data).map_err(|e| e.to_string())?;
                let _ = prov.set_attributes(&dest_str, modified, permissions);
                Ok(())
            }
            (false, false) => {
                // Should not happen for remote ops, but handle gracefully
                std::fs::copy(src_path, dest_file).map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }
}

/// Apply file attributes (modification time, permissions) to a local file.
fn apply_local_attributes(dest: &std::path::Path, modified: Option<std::time::SystemTime>, _permissions: u32) {
    if let Some(mtime) = modified {
        if let Ok(file) = std::fs::File::options().write(true).open(dest) {
            let _ = file.set_modified(mtime);
        }
    }
    #[cfg(unix)]
    if _permissions != 0 {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(_permissions));
    }
}
