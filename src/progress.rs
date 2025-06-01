use indicatif::{ProgressBar, ProgressStyle, HumanBytes};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ProgressUpdate {
    NewItemFound,
    BytesProcessed(u64),
    ErrorEncountered,
    ScanCompleted,
}

pub struct ProgressReporter {
}

impl ProgressReporter {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(
        &self,
        mut rx: mpsc::UnboundedReceiver<ProgressUpdate>,
    ) {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(120));
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["-", "\\", "|", "/"])
                .template("{spinner:.red} {msg} [{elapsed_precise}] Items: {pos}").unwrap()
        );
        pb.set_message("Scanning...");

        let mut total_items = 0u64;
        let mut total_bytes = 0u64;

        while let Some(update) = rx.recv().await {
            match update {
                ProgressUpdate::NewItemFound => {
                    total_items += 1;
                    pb.set_position(total_items);
                    pb.set_message(format!(
                        "Scanning... Items: {}, Size: {}",
                        total_items,
                        HumanBytes(total_bytes)
                    ));
                }
                ProgressUpdate::BytesProcessed(bytes) => {
                    total_bytes += bytes;
                    pb.set_message(format!(
                        "Scanning... Items: {}, Size: {}",
                        total_items,
                        HumanBytes(total_bytes)
                    ));
                }
                ProgressUpdate::ErrorEncountered => {
                    pb.set_message(format!(
                        "Scanning... Items: {}, Size: {} (errors encountered)",
                        total_items,
                        HumanBytes(total_bytes)
                    ));
                }
                ProgressUpdate::ScanCompleted => {
                    break;
                }
            }
        }

        pb.finish_with_message(format!(
            "Scan finished! Total Items: {}, Total Size: {}",
            total_items,
            HumanBytes(total_bytes)
        ));
    }
}
