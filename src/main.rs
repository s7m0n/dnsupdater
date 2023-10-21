use reqwest::Client;
//use tokio::main;
use std::error::Error;
use config::{Config, File};
use std::net::Ipv6Addr;
//use ipnetwork::IpNetwork;
use serde::Deserialize;  // Import Deserialize from serde
use dirs;

#[tokio::main]
async fn main()->Result<(), Box<dyn Error>> {
    // Get the user's home directory
    if let
        Some(home_dir) = dirs::home_dir() {
            let config_file_path = home_dir.join(".config/dnsupdaterconfig.toml");
            let mut settings = Config::default();
            settings.merge(File::from(config_file_path))?;
            let config = settings.try_into::<YourConfigStruct>()?;

            // Load configuration from the config file
            //let mut settings = Config::default();
            //settings.merge(File::with_name("config"))            ? ;
            //let config = settings.try_into::<YourConfigStruct>() ? ;

            // Get the IPv6 address of the specified interface
            let mut ip6addr = get_interface_ipv6_address(&config.interface)?;
            if let Ok(ipv6) = config.ipaddress.parse() {
                ip6addr = ipv6;
            } else {
                // Handle the parsing error here, if needed
            }

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
            //return Ok(());
            // Make the HTTPS request
            let response = client.get(&url).send().await ? ;

            // Check and handle the response
            if response
                .status().is_success() {
                    println !("Update successful! using:{} ", urlnopass);
                }
            else {
                println !("Update failed. Status: {}", response.status());
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
    // You should implement the logic to retrieve the IPv6 address from the
    // specified interface here. Replace the code below with your actual
    // implementation. For Linux, you can use external crates like `netlink` to
    // interact with the network stack.

    println!("passed interface is {}", interface_name);
    // For the sake of the example, we'll use a hardcoded IPv6 address.
    let ipv6_str = "2001:db8::1";  // Replace with the actual address.

    // Parse the IPv6 address string.
    match ipv6_str.parse() { Ok(ipv6) => Ok(ipv6), Err(e) => Err(e.into()), }
}

#[derive(Debug, Deserialize)]
struct YourConfigStruct {
    domain: String,
    interface: String,
    username: String,
    password: String,
    ipaddress: String,
    // Add more fields as needed for your configuration
}

