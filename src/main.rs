mod cli;
mod scanner;
mod progress;

use scanner::ScannerConfig;
use anyhow::Result;
use regex::Regex;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli_args = cli::parse_args();

    let default_concurrent_tasks = num_cpus::get() * 2;
    let max_concurrent_tasks = cli_args.threads.unwrap_or(default_concurrent_tasks);

    let file_pattern_regex: Option<Regex> = match cli_args.pattern {
        Some(ref pattern_str) => match Regex::new(pattern_str) {
            Ok(re) => Some(re),
            Err(e) => {
                eprintln!("Warning: Invalid regex pattern '{}': {}. Proceeding without pattern matching.", pattern_str, e);
                None
            }
        },
        None => None,
    };

    let scanner_config = ScannerConfig {
        target_path: cli_args.path.clone(),
        max_concurrent_tasks,
        follow_symlinks: cli_args.follow_symlinks,
        include_hidden: !cli_args.no_hidden,
        progress_updates: !cli_args.quiet && !cli_args.json,
        verbose: cli_args.verbose,
        file_pattern: file_pattern_regex,
    };

    println!("\nInitialized ScannerConfig: {:#?}", scanner_config);

    match scanner::run_scan(&scanner_config).await {
        Ok(scan_result) => {
            println!("\nTotal files: {}", scan_result.total_files);
            println!("Total directories: {}", scan_result.total_directories);
            println!("Scan duration: {:?}", scan_result.scan_duration);
            if !scan_result.matching_files.is_empty() {
                println!("Matching files ({}):", scan_result.matching_files.len());
                for f_path in scan_result.matching_files {
                    println!("  {:?}", f_path);
                }
            }
            if !scan_result.errors.is_empty() && cli_args.verbose {
                println!("Errors encountered ({}) :", scan_result.errors.len());
                for err in scan_result.errors {
                    println!("  - {}", err);
                }
            }
        }
        Err(e) => {
            eprintln!("\nAn error occurred during scanning: {}", e);
        }
    }
    
    Ok(())
}
