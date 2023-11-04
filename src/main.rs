use chrono::{DateTime, Local, Utc};
use clap::{App, Arg};
use config::{Config, File as ConfigFile};
use dirs;
use get_if_addrs::get_if_addrs;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{Result as IOResult, Write};
use std::net::{IpAddr, Ipv6Addr};
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};
use std::thread::sleep;

// Define a trait for the logger
trait Logger: Send + Sync {
    fn log(&self, message: &str);
}

// Implement a logger that logs to the standard output
struct StdoutLogger;

impl Logger for StdoutLogger {
    fn log(&self, message: &str) {
        println!("{}", message);
    }
}

struct FileLogger {
    file: File,
    file_path: String, // Store the file path here
}

impl FileLogger {
    fn new(file_path: &str) -> IOResult<FileLogger> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;

        Ok(FileLogger {
            file,
            file_path: file_path.to_string(), // Store the file path
        })
    }
}

impl Logger for FileLogger {
    fn log(&self, message: &str) {
        println!("Logging to file: {}", self.file_path);
        let now = Local::now();
        let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let log_line = format!("{}: {}", timestamp, message);

        if let Err(err) = writeln!(&self.file, "{}", log_line) {
            eprintln!("Error writing to log file: {}", err);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("DNS Updater")
        .version("1.0")
        .author("")
        .about("Updates DNS records with the current IP address")
        .arg(
            Arg::with_name("dry-run")
                .short("n")
                .long("dry-run")
                .help("Run in dry-run mode (no actual update)"),
        )
        .arg(
            Arg::with_name("force")
                .short("f")
                .long("force")
                .help("Force the update even if the IP address hasn't changed"),
        )
        .arg(
            Arg::with_name("demon")
                .short("d")
                .long("demon")
                .help("run as demon until stopped in background"),
        )
        .get_matches();

    let is_dry_run = matches.is_present("dry-run");
    let is_force = matches.is_present("force");
    let is_demon = matches.is_present("demon");

    let logger: Arc<Mutex<dyn Logger>>;

    if is_demon {
        // Define the log file path
        let log_file_path = "./dnsupdater.log";

        match FileLogger::new(log_file_path) {
            Ok(file_logger) => {
                logger = Arc::new(Mutex::new(file_logger));
            }
            Err(e) => {
                eprintln!("Error creating log file: {}", e);
                process::exit(1);
            }
        }
    } else {
        logger = Arc::new(Mutex::new(StdoutLogger));
    }

    let system_config_path = Path::new("/etc/dnsupdaterconfig.toml");

    let user_config = load_user_config()?;
    let system_config = load_system_config(system_config_path)?;
    let mut settings = user_config.clone();
    settings.merge(system_config)?;
    let config = settings.try_into::<YourConfigStruct>()?;

    // Read the previous IP address and timestamp from the status file
    let status_directory_path = config
        .status_file_path
        .as_ref()
        .map_or("/var/lib/cache/dnsupdater/", |path| path.as_str());

    // Combine the directory path with the "status" filename
    let combinedpath = Path::new(status_directory_path).join("status");
    let status_file_path = combinedpath
        .to_str()
        .unwrap_or("/var/lib/cache/dnsupdater/status");

    if !Path::new(&status_directory_path).exists() {
        logger
            .lock()
            .unwrap()
            .log("directory for status file doesn't exist: {status_directory_path}");
    } else if !is_writable(&status_directory_path) {
        println!(
            "No write access to status file directory: {}",
            status_directory_path
        );
    }

    let servername = config
        .server
        .as_ref()
        .map_or("update.spdyn.de", |server| server.as_str());

    let exec_config = ExecConfig {
        server_name: servername.to_string(),
        status_file_path: status_file_path.to_string(),
        domain: config.domain.to_string(),
        interface: config.interface.to_string(),
        password: config.password.to_string(),
        username: config.username.to_string(),
        is_force,
        is_dry_run,
        is_demon,
    };

    /*
    if is_demon {
        println!("Running in daemon mode.");
        logger.lock().unwrap().log("prepare daemon ...");

        let logger_clone = logger.clone();

        logger_clone.lock().unwrap().log("writing to clone");
        // Spawn a new thread for the daemon
        let _daemon_thread = std::thread::spawn(move || {
            // Daemon logic here
            logger_clone.lock().unwrap().log("Daemon is running...");
            println!("da fuck");
            match business_logic(exec_config, &logger_clone) {
                Ok(_) => {
                    // Successful execution of business_logic
                    // You can add more logic here if needed
                }
                Err(err) => {
                    // Handle the error from business_logic
                    eprintln!("Error in business logic: {:?}", err);
                }
            }
        });
        _daemon_thread.join().expect("The thread panicked");
        Ok(())
        // Exit the main thread immediately
        // std::process::exit(0);
    } else {
    */

    business_logic(exec_config, &logger)
}

fn business_logic(
    config: ExecConfig,
    logger: &Arc<Mutex<dyn Logger>>,
) -> Result<(), Box<dyn Error>> {
    loop {
        logger.lock().unwrap().log("test");

        // Get the IPv6 address of the specified interface
        let ip6addr = get_interface_ipv6_address(&config.interface)?;

        // Read the previous IP address and timestamp from the status file
        let (prev_ip, prev_time) = read_status_file(config.status_file_path.as_str())?;

        // Check if the current IP address is the same as the previous one
        if ip6addr.to_string() == prev_ip && !config.is_force {
            println!(
                "IP address has not changed since last update:{}. Skipping update.",
                prev_time
            );
        } else {
            // Build the URL with dynamic parameters
            let url = format!(
                "https://{}:{}@{}/nic/update?hostname={}&myip={}",
                config.username,
                config.password,
                config.server_name,
                config.domain,
                ip6addr.to_string()
            );
            let urlnopass = format!(
                "https://{}:{}@{}/nic/update?hostname={}&myip={}",
                config.username,
                "-hidden-",
                config.server_name,
                config.domain,
                ip6addr.to_string()
            );

            // Create an HTTP client
            let client = Client::new();

            println!("would Update using: {}", &urlnopass);

            // Make the HTTPS request
            if config.is_dry_run {
                write_status_file(config.status_file_path.as_str(), ip6addr.to_string())?;
                println!("Dry run done with updating status file!");
            } else {
                let response = client.get(&url).send()?;
                if response.status().is_success() {
                    write_status_file(config.status_file_path.as_str(), ip6addr.to_string())?;
                    println!("Update successful! using:{} ", urlnopass);
                } else {
                    println!("Update failed. Status: {}", response.status());
                    break;
                }
            }
        }

        if !config.is_demon {
            break;
        }
        // Sleep for 5 minutes
        sleep(std::time::Duration::new(300, 0));
    }

    Ok(())
}

fn get_interface_ipv6_address(interface_name: &str) -> Result<Ipv6Addr, Box<dyn Error>> {
    // Retrieve the list of network interfaces and their addresses
    let interfaces = get_if_addrs()?;

    // Iterate through all interfaces with a matching name
    for interface in interfaces
        .iter()
        .filter(|ifaddr| ifaddr.name == interface_name)
    {
        // Check if the interface has any non-local IPv6 addresses
        let ipv6_addr = interface.ip();
        if ipv6_addr.is_ipv6() && !ipv6_addr.is_loopback() {
            if let IpAddr::V6(ipv6) = ipv6_addr {
                return Ok(ipv6);
            }
        }
    }
    Err("IPv6 address not found for the specified interface".into())
}

fn load_user_config() -> Result<Config, config::ConfigError> {
    let mut config = Config::default();
    if let Some(home_dir) = dirs::home_dir() {
        let user_config_path = home_dir.join(".config/dnsupdaterconfig.toml");
        if user_config_path.exists() {
            config.merge(ConfigFile::from(user_config_path))?;
        }
    }
    Ok(config)
}
fn load_system_config(system_config_path: &Path) -> Result<Config, config::ConfigError> {
    let mut config = Config::default();
    if system_config_path.exists() {
        config.merge(ConfigFile::from(system_config_path))?;
    }
    Ok(config)
}

fn read_status_file(status_file_path: &str) -> Result<(String, DateTime<Utc>), Box<dyn Error>> {
    if let Ok(file_content) = fs::read_to_string(status_file_path) {
        if let Some((ip, timestamp_str)) = file_content.lines().next().map(|line| {
            let mut parts = line.split(',');
            let ip = parts.next().unwrap_or("");
            let timestamp_str = parts.next().unwrap_or("");
            (ip.to_string(), timestamp_str.to_string())
        }) {
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)?.with_timezone(&Utc);
            return Ok((ip, timestamp));
        }
    }
    Ok(("".to_string(), Utc::now()))
}

fn write_status_file(status_file_path: &str, ip: String) -> Result<(), Box<dyn Error>> {
    let timestamp = Utc::now();
    let status = format!("{},{}\n", ip, timestamp.to_rfc3339());

    match fs::write(status_file_path, status) {
        Ok(_) => Ok(()),
        Err(err) => {
            eprintln!(
                "Failed to write status file: {} to {}",
                status_file_path, err
            );
            Err(err.into())
        }
    }
}

fn is_writable(path: &str) -> bool {
    if let Ok(meta) = fs::metadata(path) {
        return !meta.permissions().readonly();
    }
    false
}

#[derive(Debug, Deserialize)] //
struct YourConfigStruct {
    domain: String,
    interface: String,
    username: String,
    password: String,
    status_file_path: Option<String>,
    server: Option<String>,
    // Add more fields as needed for your configuration
}

struct ExecConfig {
    server_name: String,
    status_file_path: String,
    domain: String,
    interface: String,
    password: String,
    username: String,
    is_force: bool,
    is_dry_run: bool,
    is_demon: bool,
}
