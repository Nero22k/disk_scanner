# Overview
High-performance command-line tool written in Rust that enumerates files, calculates disk usage and looks for specific files using regex pattern.

```
Usage: disk_scanner.exe [OPTIONS] <PATH>

Arguments:
  <PATH>  The path to scan

Options:
  -j, --json               Output results as JSON
  -q, --quiet              Suppress progress updates and all output except final result
  -v, --verbose            Show detailed error information
  -t, --threads <NUM>      Set concurrent task limit
      --no-hidden          Skip hidden files and directories
      --follow-symlinks    Follow symbolic links
      --timeout <SECONDS>  Maximum scan duration in seconds
  -p, --pattern <PATTERN>  Regex pattern to filter files
  -h, --help               Print help
  -V, --version            Print version
```
