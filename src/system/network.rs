use std::net::IpAddr;

use super::command;

#[derive(Debug, Clone, PartialEq)]
pub struct Wifi {
    pub connected: bool,
    pub ssid: Option<String>,
    pub signal: Option<u8>,
    pub rate_mbps: Option<u32>,
}

/// Ask NetworkManager for its cached Wi-Fi list. `--rescan no` avoids forcing
/// a radio scan on every refresh.
pub fn read() -> Option<Wifi> {
    let output = command::run(
        "nmcli",
        &[
            "-t",
            "-f",
            "IN-USE,SSID,SIGNAL,RATE",
            "dev",
            "wifi",
            "list",
            "--rescan",
            "no",
        ],
    )
    .ok()?;

    Some(parse(&output))
}

/// IPv4 address used for the default route. This is the address the machine
/// would use to reach the internet, whether the link is Wi-Fi or Ethernet.
pub fn local_ip() -> Option<String> {
    let output = command::run("ip", &["-4", "route", "get", "1.1.1.1"]).ok()?;
    parse_local_ip(&output)
}

/// Public IPv4 address, looked up over HTTPS through `curl`.
pub fn public_ip() -> Option<String> {
    let output = command::run(
        "curl",
        &["-4", "-fsS", "--max-time", "3", "https://api.ipify.org"],
    )
    .ok()?;

    parse_public_ip(&output)
}

fn parse_local_ip(output: &str) -> Option<String> {
    let mut words = output.split_whitespace();

    while let Some(word) = words.next() {
        if word == "src" {
            return words
                .next()
                .and_then(|value| value.parse::<IpAddr>().ok())
                .map(|ip| ip.to_string());
        }
    }

    None
}

fn parse_public_ip(output: &str) -> Option<String> {
    output
        .trim()
        .parse::<IpAddr>()
        .ok()
        .map(|ip| ip.to_string())
}

fn parse(output: &str) -> Wifi {
    for line in output.lines() {
        let line = line.trim_end();
        let Some((in_use, rest)) = line.split_once(':') else {
            continue;
        };
        let in_use = in_use.trim();
        if in_use != "*" && !in_use.eq_ignore_ascii_case("yes") {
            continue;
        }

        // nmcli terse mode escapes colons in the SSID. Splitting from the
        // right keeps the SSID intact.
        let mut fields = rest.rsplitn(3, ':');
        let Some(rate) = fields.next() else {
            continue;
        };
        let Some(signal) = fields.next() else {
            continue;
        };
        let Some(ssid) = fields.next() else {
            continue;
        };

        let signal = signal.parse::<u8>().ok().map(|value| value.min(100));
        let rate_mbps = rate
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u32>().ok());
        let ssid = unescape_terse(ssid);

        return Wifi {
            connected: true,
            ssid: (!ssid.is_empty()).then_some(ssid),
            signal,
            rate_mbps,
        };
    }

    Wifi {
        connected: false,
        ssid: None,
        signal: None,
        rate_mbps: None,
    }
}

fn unescape_terse(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut characters = value.chars();

    while let Some(character) = characters.next() {
        if character == '\\' {
            if let Some(escaped) = characters.next() {
                result.push(escaped);
            }
        } else {
            result.push(character);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_network_from_terse_output() {
        let output = " :Other:50:130 Mbit/s\n*:Home Net:97:130 Mbit/s\n";
        let wifi = parse(output);
        assert!(wifi.connected);
        assert_eq!(wifi.ssid.as_deref(), Some("Home Net"));
        assert_eq!(wifi.signal, Some(97));
        assert_eq!(wifi.rate_mbps, Some(130));
    }

    #[test]
    fn unescapes_colons_in_ssid() {
        let output = "*:Cafe\\:Guest:44:54 Mbit/s\n";
        let wifi = parse(output);
        assert_eq!(wifi.ssid.as_deref(), Some("Cafe:Guest"));
    }

    #[test]
    fn reports_disconnected_when_no_active_line_exists() {
        let wifi = parse(" :Other:50:130 Mbit/s\n");
        assert!(!wifi.connected);
        assert_eq!(wifi.ssid, None);
    }

    #[test]
    fn parses_local_ip_from_route_output() {
        let output = "1.1.1.1 via 192.168.1.1 dev wlan0 src 192.168.1.42 uid 1000\n";
        assert_eq!(parse_local_ip(output).as_deref(), Some("192.168.1.42"));
        assert_eq!(parse_local_ip("unreachable\n"), None);
    }

    #[test]
    fn parses_public_ip_response() {
        assert_eq!(
            parse_public_ip("203.0.113.7\n").as_deref(),
            Some("203.0.113.7")
        );
        assert_eq!(
            parse_public_ip("2001:db8::1\n").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(parse_public_ip("<html>error</html>\n"), None);
    }
}
