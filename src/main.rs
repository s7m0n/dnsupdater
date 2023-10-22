use reqwest::Client;
// use tokio::main;
use std::error::Error;
use config::{Config, File};
// use ipnetwork::IpNetwork;
use serde::Deserialize;  // Import Deserialize from serde
use dirs;
use get_if_addrs::get_if_addrs;
use std::net::{IpAddr, Ipv6Addr};
use std::fs;
//use chrono::prelude::*;
use chrono::{DateTime, FixedOffset, Utc};


#[tokio::main]
async fn main()->Result<(), Box<dyn Error>> {
    // Get the user's home directory
    if let
        Some(home_dir) = dirs::home_dir() {
            let config_file_path =
                home_dir.join(".config/dnsupdaterconfig.toml");
            let mut settings = Config::default();
            settings.merge(File::from(config_file_path))         ? ;
            let config = settings.try_into::<YourConfigStruct>() ? ;

            // Get the IPv6 address of the specified interface
            let mut ip6addr = get_interface_ipv6_address(&config.interface) ? ;
          

            // Read the previous IP address and timestamp from the status file
            let(prev_ip, prev_time) = read_status_file(home_dir.as_path())?;

            // Check if the current IP address is the same as the previous one
            if ip6addr.to_string() != prev_ip {
                    // Build the URL with dynamic parameters
                    let url = format !(
                        "https://{}:{}@update.spdyn.de/nic/update?hostname={}&myip={}",
                        config.username, config.password, config.domain,
                        ip6addr.to_string());
                    let urlnopass = format !(
                        "https://{}:{}@update.spdyn.de/nic/update?hostname={}&myip={}",
                        config.username, "-hidden-", config.domain,
                        ip6addr.to_string());

                    // Create an HTTP client
                    let client = Client::new ();

                    println !("would Update using: {}", &urlnopass);
                    // Return early and don't execute the code below
                    return Ok(());
                    // Make the HTTPS request
                    let response = client.get(&url).send().await ? ;

                    // Check and handle the response
                    if response.status().is_success() {
                            write_status_file(home_dir.as_path(), ip6addr.to_string()) ? ;
                            println !("Update successful! using:{} ",
                                      urlnopass);
                        }
                    else {
                        println !("Update failed. Status: {}",
                                  response.status());
                    }
                }
            else {
                println !("IP address has not changed. Skipping update.");
            } 
            Ok(())
        } 
    else {
        eprintln !("Failed to determine the user's home directory.");
        Ok(())
    }
}

fn get_interface_ipv6_address(interface_name
                              : &str)
    ->Result<Ipv6Addr, Box<dyn Error>> {
    // Retrieve the list of network interfaces and their addresses
    let interfaces = get_if_addrs() ? ;

    // Iterate through all interfaces with a matching name
    for
        interface in interfaces.iter().filter(| ifaddr |
                                              ifaddr.name == interface_name) {
            // Check if the interface has any non-local IPv6 addresses
            if let
                ipv6_addr = interface.ip() {
                    if ipv6_addr
                        .is_ipv6() && !ipv6_addr.is_loopback() {
                            if let
                                IpAddr::V6(ipv6) = ipv6_addr {
                                    return Ok(ipv6);
                                }
                        }
                }
        }
    Err("IPv6 address not found for the specified interface".into())
}


fn read_status_file(home_dir: &std::path::Path) -> Result<(String, DateTime<Utc>), Box<dyn Error>> {
    let status_file_path = home_dir.join(".config/dnsupdater_status.txt");
    if let Ok(file_content) = fs::read_to_string(&status_file_path) {
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

fn write_status_file(home_dir
                     : &std::path::Path, ip
                     : String)
    ->Result<(), Box<dyn Error>> {
    let status_file_path = home_dir.join(".config/dnsupdater_status.txt");
    let timestamp = Utc::now();
    let status = format !("{},{}\n", ip, timestamp.to_rfc3339());
    fs::write(status_file_path, status) ? ;
    Ok(())
}
#[derive(Debug, Deserialize)]//
struct YourConfigStruct {
    domain: String,
    interface: String,
    username: String,
    password: String,
    //ipaddress: String,
    // Add more fields as needed for your configuration
}

