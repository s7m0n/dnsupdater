use chrono::Local;
use log::{self, Level, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::{Result as IOResult, Write};

pub trait Logger: Send + Sync {
    fn log(&self, message: &str);
}

pub struct StdoutLogger;
impl Logger for StdoutLogger {
    fn log(&self, message: &str) {
        println!("{}", message);
    }
}

pub struct FileLogger {
    file: File,
    file_path: String,
}

impl FileLogger {
    pub fn new(file_path: &str) -> IOResult<FileLogger> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;

        Ok(FileLogger {
            file,
            file_path: file_path.to_string(),
        })
    }
}

impl Logger for FileLogger {
    fn log(&self, message: &str) {
        //println!("Logging to file: {}", self.file_path);
        let now = Local::now();
        let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let log_line = format!("{}: {}", timestamp, message);

        if let Err(err) = writeln!(&self.file, "{}", log_line) {
            eprintln!("Error writing to log file: {}", err);
        }
    }
}
