# dnsupdater
simple updater for spdns using dyndns (rust)

# Early stage! Use at your own risk

# usage
place and adapt dnsuptdaterconfig.toml in your /home/user/.config/ or /etc/
create a directory /var/lib/cache/dnsupdater/ which is writable for the user of the app

invoke on demand or from cron

# arguments
-h for help

-f or --force to enforce an update even if ip address did not change

-n or --dry-run to not trigger an actual update on the server (but still update status file!)

-d or --daemon to act as a daemon running until killed checking for changes (5min interval poll for now) on interface and triggering an update if needed

# configuration options in file (dnsupdaterconfig.toml)
\# Specify the domain to update

domain = "yourdomain.spdns.org"

\# Specify the network interface name to retrieve the IPv6 address

interface = "enp4s0"  # Replace with the name of your network interface

\# Username to logon to the server

username= "username.spdns.org"

\# Password to use

password = "somegoodpassww0rd"


\# optional path where status file is placed - default /var/lib/cache/dnsupdater 

\# status_file_path = "/home/user/cache/dnsupdater"

\# optional server to use for updating - default update.spdyn.de

\# server = "your.server.url"

\# optional path for logfile where output is sent in daemon mode - default /var/log/dnsupdater.log

\# logfilepath = "/var/log/dnsupdater.log"


# limitations
uses ipv6 address from local interface only (ipv4 was not in focus and might come later)

