use chrono::{DateTime, Utc};
use clap::{App, Arg};
use config::{Config, File as ConfigFile};
use dirs;
use get_if_addrs::get_if_addrs;
use logging::Logger;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::error::Error;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};
//syslog stuff
use gethostname;
extern crate syslog;
#[macro_use]
extern crate log;
use igd::aio::search_gateway;
use log::{set_boxed_logger, set_max_level, LevelFilter};
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::time::Duration;

mod logging;
macro_rules! log {
    ($logger:expr, $($arg:tt)*) => {
        $logger.lock().unwrap().log(&format!($($arg)*));
    };
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
            Arg::with_name("daemon")
                .short("d")
                .long("daemon")
                .help("run as daemon until stopped in background"),
        )
        .get_matches();

    let is_dry_run = matches.is_present("dry-run");
    let is_force = matches.is_present("force");
    let is_daemon = matches.is_present("daemon");

    let logger: Arc<Mutex<dyn Logger>>;

    // tmp syslog
    let machine_hostname = gethostname::gethostname()
        .into_string()
        .unwrap_or("unknown".to_string());

    let formatter = Formatter3164 {
        facility: Facility::LOG_DAEMON,
        hostname: Some(machine_hostname),
        process: "dnsupdater".into(),
        pid: std::process::id(),
    };

    let syslogger = syslog::unix(formatter).expect("could not connect to syslog");

    set_boxed_logger(Box::new(BasicLogger::new(syslogger)))
        .map(|()| set_max_level(LevelFilter::Info))
        .expect("Failed to set logger");

    info!("hello world");
    // end tmp syslog

    let system_config_path = Path::new("/etc/dnsupdaterconfig.toml");

    let user_config = load_user_config()?;
    let system_config = load_system_config(system_config_path)?;
    let mut settings = user_config.clone();
    settings
        .merge(system_config)
        .expect("failed to merge system with user config");
    let config = settings.try_into::<YourConfigStruct>()?;

    if is_daemon {
        let log_file_path = config
            .logfilepath
            .as_ref()
            .map_or("/var/log/dnsupdater.log", |path| path.as_str());

        match logging::FileLogger::new(log_file_path) {
            Ok(file_logger) => {
                logger = Arc::new(Mutex::new(file_logger));
            }
            Err(e) => {
                eprintln!("Error creating log file: {}", e);
                process::exit(1);
            }
        }
    } else {
        logger = Arc::new(Mutex::new(logging::StdoutLogger));
    }

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
        log!(
            logger,
            "No write access to status file directory: {}",
            status_directory_path,
        );
        error!(
            "No write access to status file directory: {}",
            status_directory_path
        );
    }

    let exec_config = ExecConfig {
        status_file_path: status_file_path.to_string(),
        interface: config.interface.to_string(),
        ddns: config.ddns,
        is_force,
        is_dry_run,
        is_daemon,
        ..Default::default()
    };

    business_logic(exec_config, &logger).await?;
    Ok(())
}

async fn update_spdyn(
    config: &ExecConfig,
    logger: &Arc<Mutex<dyn Logger>>,
) -> Result<(), Box<dyn Error>> {
    for entry in config.ddns.iter() {
        if let DdnsConfig::Spdyn {
            domain,
            server,
            username,
            password,
            name,
        } = entry
        {
            log!(
                logger,
                "Updating SPDYN for domain: {} via server: {}",
                domain,
                server
            );
            let ipv6 = &config.ipv6;
            //            let ipv4 = &config.ipv4;
            let pname = name.as_deref().unwrap_or("unnamed");
            // Build the URL with dynamic parameters
            let url = format!(
                "https://{}:{}@{}/nic/update?hostname={}&myip={}",
                username, password, server, domain, ipv6
            );
            let urlnopass = format!(
                "https://{}:{}@{}/nic/update?hostname={}&myip={}",
                username, "-hidden-", server, domain, ipv6
            );

            // Create an HTTP client
            let client = Client::new();

            log!(logger, "would Update using: {}", &urlnopass);

            // Make the HTTPS request
            if config.is_dry_run {
                //write_status_file(config.status_file_path.as_str(), ipv6, ipv4)?;
                log!(
                    logger,
                    "Dry run done for {} with updating status file!",
                    pname
                );
            } else {
                let response = client.get(&url).send().await?;
                if response.status().is_success() {
                    //write_status_file(config.status_file_path.as_str(), ipv6, ipv4)?;
                    log!(
                        logger,
                        "Update for {} successful! using:{} ",
                        pname,
                        urlnopass
                    );
                } else {
                    log!(
                        logger,
                        "Update for {} failed. Status: {}",
                        pname,
                        response.status()
                    );
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn update_cloudflare(
    config: &ExecConfig,
    logger: &Arc<Mutex<dyn Logger>>,
) -> Result<(), Box<dyn Error>> {
    for entry in config.ddns.iter() {
        if let DdnsConfig::Cloudflare {
            name,
            zone_id,
            record_name,
            record_type,
            api_token,
            proxied,
            ttl,
        } = entry
        {
            log!(
                logger,
                "Updating cloudflare for record: {} and type  {}",
                record_name,
                record_type
            );
            let ipv6 = &config.ipv6;
            let ipv4 = &config.ipv4;
            let cloudflareurl = "api.cloudflare.com/client/v4/zones/";
            let mut subrecordid = "dummy for dry runs".to_string();

            let pname = name.as_deref().unwrap_or("unnamed");

            let urlrecordid = format!(
                "https://{}/{}/dns_records?type={}&name={}",
                cloudflareurl, zone_id, record_type, record_name
            );

            let client = Client::new();

            if config.is_dry_run {
                log!(logger, "would fetch recordid using {}", urlrecordid);
            } else {
                let res = client
                    .get(urlrecordid)
                    .bearer_auth(api_token)
                    .header("Content-Type", "application/json")
                    .send()
                    .await?;

                let record_list: DnsRecordList = res.json().await?;

                subrecordid = record_list
                    .result
                    .get(0)
                    .ok_or("No matching DNS record found")?
                    .id
                    .clone();
            }

            let urlupdate = format!(
                "https://{}/{}/dns_records/{}",
                cloudflareurl, zone_id, subrecordid
            );

            let ip = match record_type.as_str() {
                "A" => ipv4,
                "AAAA" => ipv6,
                _ => {
                    return Err("unexpected record type".into());
                }
            };

            let body = json!({
                "type": record_type,
                "name": record_name,
                "content": ip,
                "ttl": ttl,
                "proxied": proxied
            });

            if config.is_dry_run {
                log!(
                    logger,
                    "would Update using: {} body: {} ",
                    &urlupdate,
                    &body
                );
            } else {
                let response = client
                    .put(&urlupdate)
                    .bearer_auth(api_token)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                // Optional: check response
                let status = response.status();
                let text = response.text().await?;

                if status.is_success() {
                    log!(
                        logger,
                        "Update for {} successful! using:{} -> {} ",
                        pname,
                        urlupdate,
                        text
                    );
                } else {
                    log!(
                        logger,
                        "Update for {} failed. Status: {} -> {}",
                        pname,
                        status,
                        text
                    );
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn business_logic(
    mut config: ExecConfig,
    logger: &Arc<Mutex<dyn Logger>>,
) -> Result<(), Box<dyn Error>> {
    loop {
        // Get the IPv6 address of the specified interface
        let ip6addr = get_interface_ipv6_address(&config.interface)?;
        let ip4addr = get_ip4_addrress().await?;
        log!(logger, "got ipv4 {}", ip4addr.to_string());

        // Read the previous IP address and timestamp from the status file
        let (prev_ip6, prev_ip4, prev_time) = read_status_file(config.status_file_path.as_str())?;

        // Check if the current IP address is the same as the previous one
        if ip6addr.to_string() == prev_ip6 && ip4addr.to_string() == prev_ip4 && !config.is_force {
            log!(
                logger,
                "IP address has not changed since last update:{}. Skipping update.",
                prev_time
            );
        } else {
            log!(
                logger,
                "IP changed old/new: v6:{}/{} v4:{}/{}   - prepare update!",
                prev_ip6,
                ip6addr.to_string(),
                prev_ip4,
                ip4addr.to_string()
            );
            config.ipv4 = ip4addr.to_string();
            config.ipv6 = ip6addr.to_string();

            update_spdyn(&config, logger).await?;
            update_cloudflare(&config, logger).await?;

            write_status_file(
                config.status_file_path.as_str(),
                &ip6addr.to_string(),
                &ip4addr.to_string(),
            )?;
            log!(
                logger,
                "Update to ipv4:{} ipv6:{} successful!",
                ip4addr,
                ip6addr,
            );
        }

        if !config.is_daemon {
            break;
        }
        // Sleep for 50 minutes
        tokio::time::sleep(Duration::from_secs(3000)).await;
    }

    Ok(())
}

async fn get_ip4_addrress() -> Result<Ipv4Addr, Box<dyn Error>> {
    let gateway = search_gateway(Default::default())
        .await
        .map_err(|e| format!("Failed to find gateway: {}", e))?;

    let ip = gateway
        .get_external_ip()
        .await
        .map_err(|e| format!("Failed to get external IP: {}", e))?;

    //println!("External IPv4: {}", ip);

    Ok(ip)
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

fn load_user_config() -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config::default();
    if let Some(home_dir) = dirs::home_dir() {
        let user_config_path = home_dir.join(".config/dnsupdaterconfig.toml");
        if user_config_path.exists() {
            config.merge(ConfigFile::from(user_config_path))?;
        }
    }
    Ok(config)
}

fn load_system_config(system_config_path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config::default();
    if system_config_path.exists() {
        config.merge(ConfigFile::from(system_config_path))?;
    }
    Ok(config)
}

fn read_status_file(
    status_file_path: &str,
) -> Result<(String, String, DateTime<Utc>), Box<dyn Error>> {
    if let Ok(file_content) = fs::read_to_string(status_file_path) {
        if let Some((ip6, ip4, timestamp_str)) = file_content.lines().next().map(|line| {
            let mut parts = line.split(',');
            let ip6 = parts.next().unwrap_or("");
            let ip4 = parts.next().unwrap_or("");
            let timestamp_str = parts.next().unwrap_or("");
            (ip6.to_string(), ip4.to_string(), timestamp_str.to_string())
        }) {
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)?.with_timezone(&Utc);
            return Ok((ip6, ip4, timestamp));
        }
    }
    Ok(("".to_string(), "".to_string(), Utc::now()))
}

fn write_status_file(
    status_file_path: &str,
    ip6: &String,
    ip4: &String,
) -> Result<(), Box<dyn Error>> {
    let timestamp = Utc::now();
    let status = format!("{},{},{}\n", ip6, ip4, timestamp.to_rfc3339());

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
    interface: String,
    pub ddns: Vec<DdnsConfig>,
    status_file_path: Option<String>,
    logfilepath: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ExecConfig {
    status_file_path: String,
    interface: String,
    #[serde(default)]
    ipv4: String,
    #[serde(default)]
    ipv6: String,
    pub ddns: Vec<DdnsConfig>,
    is_force: bool,
    is_dry_run: bool,
    is_daemon: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "provider")] // to match config dynamically
pub enum DdnsConfig {
    Spdyn {
        name: Option<String>,
        server: String,
        domain: String,
        username: String,
        password: String,
    },
    Cloudflare {
        name: Option<String>,
        zone_id: String,
        record_name: String,
        record_type: String, // "A" or "AAAA"
        api_token: String,
        proxied: Option<bool>,
        ttl: Option<u32>,
    },
}

#[derive(Debug, Deserialize)]
struct DnsRecordList {
    result: Vec<DnsRecord>,
}

#[derive(Debug, Deserialize)]
struct DnsRecord {
    id: String,
    // not yet needing others
}
