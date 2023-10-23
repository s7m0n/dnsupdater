# dnsupdater
simple updater for spdns using dyndns (rust)

# Early stage! Use at your own risk

# usage
place and adapt dnsuptdaterconfig.toml in your /home/user/.config/ or /etc/
create a directory /var/cache/dnsupdater/ which is writable for the user of the app

invoke on demand or from cron

# arguments
-h for help

-f or --force to enforce an update even if ip address did not change

-n or --dry-run to not trigger an actual update on the server (but still update status file!)

# configuration options in file (dnsupdaterconfig.toml)
\# Specify the domain to update

domain = "yourdomain.spdns.org"

\# Specify the network interface name to retrieve the IPv6 address

interface = "enp4s0"  # Replace with the name of your network interface

\# Username to logon to the server

username= "username.spdns.org"

\# Password to use

password = "somegoodpassww0rd"


\# optional path where status file is placed - default /var/cache/dnsupdater 

\# status_file_path = "/home/user/cache/dnsupdater"

# limitations
uses ipv6 address from local interface only (ipv4 was not in focus and might come later)

