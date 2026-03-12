use std::{io, net::IpAddr};

use tokio::{
    net::lookup_host,
    time::{Duration, sleep, timeout},
};

const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);

/// Return true if the IP falls in a private / loopback / link-local range.
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unicast_link_local()
                || v6.is_unique_local()
                || v6.is_unspecified()
        }
    }
}

/// Resolve `host` to an IP and reject if private (SSRF prevention).
///
/// Retries transient DNS errors up to 3 times with exponential backoff.
pub async fn resolve_and_check(host: &str) -> Result<IpAddr, Box<dyn std::error::Error>> {
    let mut backoff = Duration::from_millis(250);
    let mut last_err: Option<io::Error> = None;

    for attempt in 1..=3 {
        match timeout(DNS_LOOKUP_TIMEOUT, lookup_host((host, 443))).await {
            Ok(Ok(addrs)) => {
                let mut saw_any = false;
                for addr in addrs {
                    saw_any = true;
                    let ip = addr.ip();
                    if !is_private(ip) {
                        return Ok(ip);
                    }
                }

                if saw_any {
                    return Err(format!("all resolved IPs for {host} were private").into());
                }

                return Err(format!("no IP addresses resolved for {host}").into());
            }
            Ok(Err(e)) => {
                last_err = Some(e);
                if attempt < 3 {
                    sleep(backoff).await;
                    backoff *= 2;
                    continue;
                }
            }
            Err(_) => {
                last_err = Some(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("DNS lookup timed out for {host}"),
                ));
                if attempt < 3 {
                    sleep(backoff).await;
                    backoff *= 2;
                    continue;
                }
            }
        }
    }

    Err(last_err
        .unwrap_or_else(|| io::Error::other("DNS resolution failed"))
        .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_is_private() {
        assert!(is_private("127.0.0.1".parse().unwrap()));
        assert!(is_private("::1".parse().unwrap()));
    }

    #[test]
    fn public_ip_is_not_private() {
        assert!(!is_private("1.1.1.1".parse().unwrap()));
    }
}
