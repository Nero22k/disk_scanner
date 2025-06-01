use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::{mpsc, Semaphore};
use thiserror::Error;
use std::future::Future;
use std::pin::Pin;
use regex::Regex;

use crate::progress::{ProgressUpdate, ProgressReporter};

#[derive(Debug, Clone)]
pub struct ScannerConfig {
    pub target_path: PathBuf,
    pub max_concurrent_tasks: usize,
    pub follow_symlinks: bool,
    pub include_hidden: bool,
    pub progress_updates: bool,
    pub verbose: bool,
    pub file_pattern: Option<Regex>,
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("I/O error accessing {path:?}: {source}")]
    IoError { path: PathBuf, source: std::io::Error },

    #[error("Path is not a directory: {path:?}")]
    NotADirectory { path: PathBuf },

    #[error("Failed to read metadata for {path:?}: {source}")]
    MetadataError { path: PathBuf, source: std::io::Error },
}

#[derive(Debug)]
pub struct ScanResult {
    pub total_files: u64,
    pub total_directories: u64,
    pub total_size: u64,
    pub scan_duration: Duration,
    pub errors: Vec<ScanError>,
    pub matching_files: Vec<PathBuf>,
}

fn walk_directory_recursive(
    current_path: PathBuf,
    config: Arc<ScannerConfig>,
    semaphore: Arc<Semaphore>,
    progress_tx: Option<mpsc::UnboundedSender<ProgressUpdate>>,
) -> Pin<Box<dyn Future<Output = (u64, u64, u64, Vec<ScanError>, Vec<PathBuf>)> + Send + 'static>> {
    Box::pin(async move {
        let permit = match semaphore.acquire().await { // Acquire semaphore
            Ok(p) => p,
            Err(_) => return (0, 0, 0, vec![], vec![]),
        };

        if config.verbose {
            println!("[VERBOSE] Reading directory (permit acquired): {:?}", &current_path);
        }

        let mut files_count = 0;
        let mut dirs_count = 0;
        let mut current_size = 0;
        let mut errors = Vec::new();
        let mut sub_task_paths_to_spawn = Vec::new();
        let mut matching_files_in_dir = Vec::new();

        let mut entries_reader = match fs::read_dir(&current_path).await {
            Ok(reader) => reader,
            Err(e) => {
                errors.push(ScanError::IoError { path: current_path.clone(), source: e });
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(ProgressUpdate::ErrorEncountered);
                }
                return (files_count, dirs_count, current_size, errors, matching_files_in_dir);
            }
        };

        while let Some(entry_result) = match entries_reader.next_entry().await {
            Ok(Some(entry)) => Some(Ok(entry)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        } {
            let entry = match entry_result {
                Ok(entry) => entry,
                Err(e) => {
                    errors.push(ScanError::IoError { path: current_path.clone(), source: e });
                    if let Some(tx) = &progress_tx {
                        let _ = tx.send(ProgressUpdate::ErrorEncountered);
                    }
                    continue;
                }
            };

            let path = entry.path();

            if config.verbose {
                println!("[VERBOSE] Processing entry: {:?}", &path);
            }

            if !config.include_hidden {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.starts_with('.') && file_name != "." && file_name != ".." {
                        continue;
                    }
                }
            }

            let entry_file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    errors.push(ScanError::IoError { path: path.clone(), source: e });
                    if let Some(tx) = &progress_tx {
                        let _ = tx.send(ProgressUpdate::ErrorEncountered);
                    }
                    continue;
                }
            };

            if entry_file_type.is_symlink() {
                if config.follow_symlinks {
                    match fs::metadata(&path).await {
                        Ok(target_metadata) => {
                            if target_metadata.is_file() {
                                files_count += 1;
                                current_size += target_metadata.len();
                                if let Some(tx) = &progress_tx {
                                    let _ = tx.send(ProgressUpdate::NewItemFound);
                                    let _ = tx.send(ProgressUpdate::BytesProcessed(target_metadata.len()));
                                }
                            } else if target_metadata.is_dir() {
                                dirs_count += 1;
                                if let Some(tx) = &progress_tx {
                                    let _ = tx.send(ProgressUpdate::NewItemFound);
                                }
                                sub_task_paths_to_spawn.push(path.clone());
                            }
                        }
                        Err(e) => {
                            errors.push(ScanError::MetadataError { path, source: e });
                            if let Some(tx) = &progress_tx {
                                let _ = tx.send(ProgressUpdate::ErrorEncountered);
                            }
                        }
                    }
                }
            } else if entry_file_type.is_file() {
                match entry.metadata().await {
                    Ok(metadata) => {
                        files_count += 1;
                        current_size += metadata.len();
                        if let Some(tx) = &progress_tx {
                            let _ = tx.send(ProgressUpdate::NewItemFound);
                            let _ = tx.send(ProgressUpdate::BytesProcessed(metadata.len()));
                        }

                        // Check for regex pattern match
                        if let Some(pattern) = &config.file_pattern {
                            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                                if pattern.is_match(file_name) {
                                    matching_files_in_dir.push(path.clone());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        errors.push(ScanError::MetadataError { path, source: e });
                        if let Some(tx) = &progress_tx {
                            let _ = tx.send(ProgressUpdate::ErrorEncountered);
                        }
                    }
                }
            } else if entry_file_type.is_dir() {
                dirs_count += 1;
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(ProgressUpdate::NewItemFound);
                }
                sub_task_paths_to_spawn.push(path.clone());
            }
        }

        if config.verbose {
            println!("[VERBOSE] Releasing permit for: {:?}, collected {} sub-paths to spawn", &current_path, sub_task_paths_to_spawn.len());
        }
        drop(permit); // If we don't drop the permit, the semaphore will never release causing a deadlock

        let mut tasks = Vec::new();
        for sub_path in sub_task_paths_to_spawn {
            if config.verbose {
                println!("[VERBOSE] Spawning task for sub-path: {:?} (parent: {:?})", &sub_path, &current_path);
            }
            let task_config = Arc::clone(&config);
            let task_semaphore = Arc::clone(&semaphore);
            let task_progress_tx = progress_tx.clone();
            tasks.push(tokio::spawn(walk_directory_recursive(
                sub_path,
                task_config,
                task_semaphore,
                task_progress_tx,
            )));
        }

        for task_handle in tasks {
            match task_handle.await {
                Ok((sub_files, sub_dirs, sub_size, sub_errors, sub_matching_files)) => {
                    files_count += sub_files;
                    dirs_count += sub_dirs;
                    current_size += sub_size;
                    errors.extend(sub_errors);
                    matching_files_in_dir.extend(sub_matching_files);
                }
                Err(join_error) => {
                    eprintln!("Task panicked or was cancelled for a sub-path of {:?}: {:?}", &current_path, join_error);
                    if let Some(tx) = &progress_tx {
                        let _ = tx.send(ProgressUpdate::ErrorEncountered);
                    }
                }
            }
        }
        (files_count, dirs_count, current_size, errors, matching_files_in_dir)
    })
}

/// Scanner Engine
/// - Walks a directory tree recursively
/// - Reports progress using a channel
/// - Returns a ScanResult
/// 
pub async fn run_scan(config: &ScannerConfig) -> Result<ScanResult, anyhow::Error> {
    let start_time = Instant::now();

    let root_path = PathBuf::from(&config.target_path);
    // Check if the root path is a directory
    match fs::metadata(&root_path).await {
        Ok(meta) => {
            if !meta.is_dir() {
                return Err(ScanError::NotADirectory { path: root_path }.into());
            }
        }
        Err(e) => {
            return Err(ScanError::IoError{ path: root_path, source: e }.into());
        }
    }

    let (progress_tx, progress_rx) = mpsc::unbounded_channel();
    let mut progress_reporter_handle = None;

    if config.progress_updates {
        let reporter = ProgressReporter::new();
        progress_reporter_handle = Some(tokio::spawn(async move {
            reporter.run(progress_rx).await;
        }));
    } else {
        // Drop the receiver if not used, so sender doesn't wait indefinitely or panic.
        drop(progress_rx);
    }
    
    let progress_tx_option = if config.progress_updates { Some(progress_tx) } else { None };

    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_tasks));
    let arc_config = Arc::new(config.clone());

    // Send initial NewItemFound for the root directory itself if progress is enabled
    if let Some(tx) = &progress_tx_option {
        let _ = tx.send(ProgressUpdate::NewItemFound);
    }

    let (files, sub_dirs, size, scan_errors, matching_files) = walk_directory_recursive(
        root_path,
        arc_config,
        semaphore,
        progress_tx_option.clone(),
    ).await;

    // Signal scan completion
    if let Some(tx) = progress_tx_option {
        let _ = tx.send(ProgressUpdate::ScanCompleted);
        if let Some(handle) = progress_reporter_handle {
            let _ = handle.await;
        }
    }

    let scan_duration = start_time.elapsed();

    let result = ScanResult {
        total_files: files,
        total_directories: sub_dirs + 1, 
        total_size: size,
        scan_duration,
        errors: scan_errors,
        matching_files,
    };

    if !config.progress_updates {
        println!("Scanner Engine: Scan complete.");
    }
    Ok(result)
}
