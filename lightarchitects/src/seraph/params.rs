//! Typed parameter builders for SERAPH wing actions.
//!
//! Each struct maps to the `params` field of a `penTools` JSON-RPC call.
//! Callers fill in the struct using builder methods; [`crate::seraph::SeraphClient`]
//! serializes it via the `_typed` methods (e.g. `scan_typed`, `capture_typed`).
//!
//! All optional fields use `skip_serializing_if = "Option::is_none"` so only
//! explicitly set values are sent to SERAPH.

use serde::Serialize;

// ── ScanParams ──────────────────────────────────────────────────────────────

/// Parameters for the `scan` wing (Wing 3: recon / vuln).
#[derive(Debug, Clone, Serialize)]
pub struct ScanParams {
    /// IP address, CIDR range, or hostname to scan.
    pub target: String,
    /// Scan type. Defaults to `"port"` if omitted.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub scan_type: Option<ScanType>,
    /// Port specification: `"80,443"` or `"1-1000"`. Defaults to top-1000.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<String>,
    /// Nmap timing template 0-5. Defaults to 3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<u8>,
    /// Explicit tool override (`"nmap"`, `"masscan"`, `"fping"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// Scan type selector for the scan wing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanType {
    /// TCP port scan (default).
    Port,
    /// Service/version detection.
    Service,
    /// Vulnerability scan (nmap --script vuln).
    Vuln,
    /// ICMP / ARP ping sweep.
    Ping,
    /// UDP port scan.
    Udp,
}

impl ScanParams {
    /// Create a minimal scan with just a target (all other fields defaulted).
    #[must_use]
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            scan_type: None,
            ports: None,
            timing: None,
            tool: None,
        }
    }

    /// Set the scan type.
    #[must_use]
    pub fn with_type(mut self, scan_type: ScanType) -> Self {
        self.scan_type = Some(scan_type);
        self
    }

    /// Set the port specification.
    #[must_use]
    pub fn with_ports(mut self, ports: impl Into<String>) -> Self {
        self.ports = Some(ports.into());
        self
    }

    /// Set the Nmap timing template (0-5).
    #[must_use]
    pub fn with_timing(mut self, timing: u8) -> Self {
        self.timing = Some(timing);
        self
    }

    /// Set an explicit tool override.
    #[must_use]
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }
}

// ── CaptureParams ───────────────────────────────────────────────────────────

/// Parameters for the `capture` wing (Wing 2: packet capture).
#[derive(Debug, Clone, Serialize)]
pub struct CaptureParams {
    /// Network interface to capture on (e.g. `"eth0"`, `"lo"`).
    pub interface: String,
    /// Capture duration in seconds. Defaults to 10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    /// Maximum packet count limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    /// BPF filter expression (e.g. `"tcp port 80"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Output `.pcap` file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Explicit tool override (`"tcpdump"`, `"tshark"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

impl CaptureParams {
    /// Create capture params for the given interface.
    #[must_use]
    pub fn new(interface: impl Into<String>) -> Self {
        Self {
            interface: interface.into(),
            duration: None,
            count: None,
            filter: None,
            output: None,
            tool: None,
        }
    }

    /// Set the capture duration.
    #[must_use]
    pub fn with_duration(mut self, secs: u64) -> Self {
        self.duration = Some(secs);
        self
    }

    /// Set a BPF filter expression.
    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Set the maximum packet count.
    #[must_use]
    pub fn with_count(mut self, count: u64) -> Self {
        self.count = Some(count);
        self
    }

    /// Set the output `.pcap` file path.
    #[must_use]
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Set an explicit tool override.
    #[must_use]
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }
}

// ── AnalyzeParams ───────────────────────────────────────────────────────────

/// Parameters for the `analyze` wing (Wing 4: forensics).
#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeParams {
    /// Absolute path to the file to analyze.
    pub target: String,
    /// Analysis type. Defaults to `"metadata"` if omitted.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub analyze_type: Option<AnalyzeType>,
    /// Path to YARA rules file (required when `analyze_type = Yara`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
    /// Explicit tool override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// Analysis type selector for the analyze wing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalyzeType {
    /// YARA rule matching.
    Yara,
    /// File metadata via exiftool (default).
    Metadata,
    /// Firmware/binary extraction via binwalk.
    Binwalk,
    /// String extraction via `strings`.
    Strings,
    /// Interactive disassembly via radare2.
    Radare2,
}

impl AnalyzeParams {
    /// Create analyze params for the given file path.
    #[must_use]
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            analyze_type: None,
            rules: None,
            tool: None,
        }
    }

    /// Set the analysis type.
    #[must_use]
    pub fn with_type(mut self, analyze_type: AnalyzeType) -> Self {
        self.analyze_type = Some(analyze_type);
        self
    }

    /// Set a YARA rules file path (required for `AnalyzeType::Yara`).
    #[must_use]
    pub fn with_rules(mut self, rules: impl Into<String>) -> Self {
        self.rules = Some(rules.into());
        self
    }

    /// Set an explicit tool override.
    #[must_use]
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }
}

// ── OsintParams ─────────────────────────────────────────────────────────────

/// Parameters for the `osint` wing (Wing 5: OSINT).
#[derive(Debug, Clone, Serialize)]
pub struct OsintParams {
    /// Domain name or IP to investigate.
    pub target: String,
    /// Caller attestation that this target is authorized for OSINT.
    /// Must be `true` or the wing returns a scope violation.
    pub authorized: bool,
    /// OSINT type. Defaults to `"subdomain"` if omitted.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub osint_type: Option<OsintType>,
    /// Wing timeout in seconds. Defaults to 120.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// Explicit tool override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// OSINT type selector for the osint wing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OsintType {
    /// Subdomain enumeration (default).
    Subdomain,
    /// DNS record lookup.
    Dns,
    /// HTTP probing.
    Http,
    /// Web crawl.
    Crawl,
    /// Shodan host intelligence lookup (IP only). Requires `SHODAN_API_KEY`.
    Shodan,
    /// `VirusTotal` reputation lookup (IP or domain). Requires `VIRUSTOTAL_API_KEY`.
    Virustotal,
    /// Censys host data lookup (IP only). Requires `CENSYS_API_ID` + `CENSYS_API_SECRET`.
    Censys,
    /// `GreyNoise` community IP classification (IP only). Requires `GREYNOISE_API_KEY`.
    Greynoise,
    /// `AbuseIPDB` IP reputation check (IP only). Requires `ABUSEIPDB_API_KEY`.
    Abuseipdb,
}

impl OsintParams {
    /// Create OSINT params, requiring explicit `authorized: true`.
    #[must_use]
    pub fn new(target: impl Into<String>, authorized: bool) -> Self {
        Self {
            target: target.into(),
            authorized,
            osint_type: None,
            timeout_secs: None,
            tool: None,
        }
    }

    /// Set the OSINT type.
    #[must_use]
    pub fn with_type(mut self, osint_type: OsintType) -> Self {
        self.osint_type = Some(osint_type);
        self
    }

    /// Set the wing timeout in seconds.
    #[must_use]
    pub fn with_timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Set an explicit tool override.
    #[must_use]
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }
}

// ── MonitorParams ───────────────────────────────────────────────────────────

/// Parameters for the `monitor` wing (Wing 6: IDS / network monitoring).
#[derive(Debug, Clone, Serialize)]
pub struct MonitorParams {
    /// Action to perform.
    #[serde(rename = "action")]
    pub monitor_action: MonitorAction,
    /// Network interface (required for `arp-watch`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// Explicit tool override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// Monitor action selector for the monitor wing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MonitorAction {
    /// List network interfaces.
    Interfaces,
    /// Watch ARP table for anomalies (read-only).
    ArpWatch,
    /// Query Suricata build info / status.
    IdsStatus,
}

impl MonitorParams {
    /// Create monitor params for the given action.
    #[must_use]
    pub fn new(monitor_action: MonitorAction) -> Self {
        Self {
            monitor_action,
            interface: None,
            tool: None,
        }
    }

    /// Set the interface (required for `MonitorAction::ArpWatch`).
    #[must_use]
    pub fn with_interface(mut self, iface: impl Into<String>) -> Self {
        self.interface = Some(iface.into());
        self
    }

    /// Set an explicit tool override.
    #[must_use]
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_params_serialize_minimal() {
        let params = ScanParams::new("192.168.1.1");
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["target"], "192.168.1.1");
        assert!(json.get("type").is_none());
        assert!(json.get("ports").is_none());
    }

    #[test]
    fn scan_params_serialize_full() {
        let params = ScanParams::new("10.0.0.0/24")
            .with_type(ScanType::Service)
            .with_ports("80,443")
            .with_timing(4)
            .with_tool("nmap");
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["target"], "10.0.0.0/24");
        assert_eq!(json["type"], "service");
        assert_eq!(json["ports"], "80,443");
        assert_eq!(json["timing"], 4);
        assert_eq!(json["tool"], "nmap");
    }

    #[test]
    fn capture_params_serialize() {
        let params = CaptureParams::new("eth0")
            .with_duration(30)
            .with_filter("tcp port 443");
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["interface"], "eth0");
        assert_eq!(json["duration"], 30);
        assert_eq!(json["filter"], "tcp port 443");
    }

    #[test]
    fn analyze_params_serialize() {
        let params = AnalyzeParams::new("/tmp/sample.exe")
            .with_type(AnalyzeType::Yara)
            .with_rules("/rules/malware.yar");
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["target"], "/tmp/sample.exe");
        assert_eq!(json["type"], "yara");
        assert_eq!(json["rules"], "/rules/malware.yar");
    }

    #[test]
    fn osint_params_serialize() {
        let params = OsintParams::new("example.com", true).with_type(OsintType::Shodan);
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["target"], "example.com");
        assert_eq!(json["authorized"], true);
        assert_eq!(json["type"], "shodan");
    }

    #[test]
    fn osint_all_types_serialize() {
        let types_expected = [
            (OsintType::Subdomain, "subdomain"),
            (OsintType::Dns, "dns"),
            (OsintType::Http, "http"),
            (OsintType::Crawl, "crawl"),
            (OsintType::Shodan, "shodan"),
            (OsintType::Virustotal, "virustotal"),
            (OsintType::Censys, "censys"),
            (OsintType::Greynoise, "greynoise"),
            (OsintType::Abuseipdb, "abuseipdb"),
        ];
        for (osint_type, expected) in types_expected {
            let params = OsintParams::new("target", true).with_type(osint_type);
            let json = serde_json::to_value(&params).unwrap();
            assert_eq!(json["type"], expected);
        }
    }

    #[test]
    fn monitor_params_serialize() {
        let params = MonitorParams::new(MonitorAction::ArpWatch).with_interface("eth0");
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["action"], "arp-watch");
        assert_eq!(json["interface"], "eth0");
    }

    #[test]
    fn monitor_all_actions_serialize() {
        let actions_expected = [
            (MonitorAction::Interfaces, "interfaces"),
            (MonitorAction::ArpWatch, "arp-watch"),
            (MonitorAction::IdsStatus, "ids-status"),
        ];
        for (action, expected) in actions_expected {
            let params = MonitorParams::new(action);
            let json = serde_json::to_value(&params).unwrap();
            assert_eq!(json["action"], expected);
        }
    }
}
