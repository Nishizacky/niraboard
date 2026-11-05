use anyhow::{Result, anyhow};
use serde_json::Value;

use super::command;

/// The local Bluetooth controller as reported by BlueZ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    /// D-Bus object path, for example `/org/bluez/hci0`.
    pub path: String,
    pub address: String,
    pub alias: Option<String>,
    pub powered: bool,
    pub discoverable: bool,
    pub discovering: bool,
}

impl Adapter {
    pub fn label(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.address)
    }
}

/// A device known to BlueZ. Devices are included whether they are paired,
/// currently connected, or were only seen during a scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// D-Bus object path, for example
    /// `/org/bluez/hci0/dev_E8_EE_CC_66_73_58`.
    pub path: String,
    pub address: String,
    pub name: Option<String>,
    pub connected: bool,
}

impl Device {
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.address)
    }
}

/// A snapshot of the default Bluetooth adapter and the devices known to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bluetooth {
    pub adapter: Option<Adapter>,
    pub devices: Vec<Device>,
}

impl Bluetooth {
    pub fn connected_count(&self) -> usize {
        self.devices
            .iter()
            .filter(|device| device.connected)
            .count()
    }
}

/// Read the default adapter and its device list through BlueZ's D-Bus API.
///
/// `GetManagedObjects` returns the adapter and every known device in one
/// round-trip. A missing adapter is not an error: BlueZ can be running on a
/// machine without Bluetooth hardware. A missing or unreachable bus is an
/// error so the UI can distinguish "no adapter" from "service unavailable".
pub fn read() -> Result<Bluetooth> {
    let output = command::run(
        "busctl",
        &[
            "--system",
            "--json=short",
            "--timeout=5",
            "call",
            "org.bluez",
            "/",
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
        ],
    )?;

    parse_managed_objects(&output)
}

fn parse_managed_objects(output: &str) -> Result<Bluetooth> {
    let document: Value = serde_json::from_str(output)?;
    let objects = document
        .get("data")
        .and_then(Value::as_array)
        .and_then(|data| data.first())
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("unexpected response from org.bluez"))?;

    let mut adapters = Vec::new();

    for (path, interfaces) in objects {
        let Some(properties) = interface_properties(interfaces, "org.bluez.Adapter1") else {
            continue;
        };

        adapters.push(Adapter {
            path: path.clone(),
            address: property_string(properties, "Address").unwrap_or_default(),
            alias: property_string(properties, "Alias"),
            powered: property_bool(properties, "Powered").unwrap_or(false),
            discoverable: property_bool(properties, "Discoverable").unwrap_or(false),
            discovering: property_bool(properties, "Discovering").unwrap_or(false),
        });
    }

    // BlueZ normally exposes exactly one default controller. Sorting by path
    // makes hci0 win if more than one is present.
    adapters.sort_by(|left, right| left.path.cmp(&right.path));
    let Some(adapter) = adapters.into_iter().next() else {
        return Ok(Bluetooth::default());
    };

    let device_prefix = format!("{}/dev_", adapter.path);
    let mut devices = Vec::new();

    for (path, interfaces) in objects {
        if !path.starts_with(&device_prefix) {
            continue;
        }
        let Some(properties) = interface_properties(interfaces, "org.bluez.Device1") else {
            continue;
        };
        let Some(address) = property_string(properties, "Address") else {
            continue;
        };

        devices.push(Device {
            path: path.clone(),
            address: address.to_ascii_uppercase(),
            name: property_string(properties, "Alias")
                .or_else(|| property_string(properties, "Name")),
            connected: property_bool(properties, "Connected").unwrap_or(false),
        });
    }

    // Connected devices are the most useful, then devices by name.
    devices.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then_with(|| {
                left.label()
                    .to_lowercase()
                    .cmp(&right.label().to_lowercase())
            })
            .then_with(|| left.address.cmp(&right.address))
    });

    Ok(Bluetooth {
        adapter: Some(adapter),
        devices,
    })
}

fn interface_properties<'a>(
    interfaces: &'a Value,
    interface: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    interfaces.as_object()?.get(interface)?.as_object()
}

fn property_value<'a>(
    properties: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Option<&'a Value> {
    properties.get(name)?.get("data")
}

fn property_string(properties: &serde_json::Map<String, Value>, name: &str) -> Option<String> {
    let value = property_value(properties, name)?.as_str().map(clean)?;
    (!value.is_empty()).then_some(value)
}

fn property_bool(properties: &serde_json::Map<String, Value>, name: &str) -> Option<bool> {
    property_value(properties, name)?.as_bool()
}

/// Device names and aliases are external input. Drop control characters,
/// including terminal escape sequences, so they cannot disturb the layout.
fn clean(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\n' | '\r' | '\t' => cleaned.push(' '),
            character if character.is_control() => {}
            character => cleaned.push(character),
        }
    }

    cleaned.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "type": "a{oa{sa{sv}}}",
        "data": [{
            "/org/bluez/hci0": {
                "org.bluez.Adapter1": {
                    "Address": {"type": "s", "data": "4C:EB:BD:6A:B7:AA"},
                    "Alias": {"type": "s", "data": "laptop"},
                    "Powered": {"type": "b", "data": true},
                    "Discoverable": {"type": "b", "data": false},
                    "Discovering": {"type": "b", "data": true}
                }
            },
            "/org/bluez/hci0/dev_00_11_22_33_44_55": {
                "org.bluez.Device1": {
                    "Address": {"type": "s", "data": "00:11:22:33:44:55"},
                    "Alias": {"type": "s", "data": "Headphones"},
                    "Connected": {"type": "b", "data": false}
                }
            },
            "/org/bluez/hci0/dev_66_77_88_99_AA_BB": {
                "org.bluez.Device1": {
                    "Address": {"type": "s", "data": "66:77:88:99:AA:BB"},
                    "Name": {"type": "s", "data": "Keyboard"},
                    "Connected": {"type": "b", "data": true}
                }
            }
        }]
    }"#;

    #[test]
    fn parses_adapter_and_device_objects() {
        let bluetooth = parse_managed_objects(SAMPLE).expect("bluez objects");
        let adapter = bluetooth.adapter.expect("adapter");

        assert_eq!(adapter.path, "/org/bluez/hci0");
        assert_eq!(adapter.address, "4C:EB:BD:6A:B7:AA");
        assert_eq!(adapter.alias.as_deref(), Some("laptop"));
        assert!(adapter.powered);
        assert!(!adapter.discoverable);
        assert!(adapter.discovering);

        assert_eq!(bluetooth.devices.len(), 2);
        assert_eq!(bluetooth.devices[0].address, "66:77:88:99:AA:BB");
        assert_eq!(
            bluetooth.devices[0].path,
            "/org/bluez/hci0/dev_66_77_88_99_AA_BB"
        );
        assert_eq!(bluetooth.devices[0].name.as_deref(), Some("Keyboard"));
        assert!(bluetooth.devices[0].connected);
        assert_eq!(bluetooth.devices[1].name.as_deref(), Some("Headphones"));
        assert!(!bluetooth.devices[1].connected);
    }

    #[test]
    fn alias_takes_precedence_over_the_remote_name() {
        let input = SAMPLE.replace(
            r#""Name": {"type": "s", "data": "Keyboard"}"#,
            r#""Alias": {"type": "s", "data": "Desk Keys"}"#,
        );
        let bluetooth = parse_managed_objects(&input).expect("bluez objects");

        assert_eq!(bluetooth.devices[0].name.as_deref(), Some("Desk Keys"));
    }

    #[test]
    fn no_adapter_is_not_an_error() {
        let input = r#"{"type": "a{oa{sa{sv}}}", "data": [{}]}"#;
        let bluetooth = parse_managed_objects(input).expect("empty bluez objects");

        assert_eq!(bluetooth, Bluetooth::default());
    }

    #[test]
    fn devices_from_other_adapters_are_ignored() {
        let input = SAMPLE.replace(
            "/org/bluez/hci0/dev_00_11_22_33_44_55",
            "/org/bluez/hci1/dev_00_11_22_33_44_55",
        );
        let bluetooth = parse_managed_objects(&input).expect("bluez objects");

        assert_eq!(bluetooth.devices.len(), 1);
        assert_eq!(bluetooth.devices[0].address, "66:77:88:99:AA:BB");
    }

    #[test]
    fn malformed_busctl_output_is_an_error() {
        assert!(parse_managed_objects("not json").is_err());
        assert!(parse_managed_objects(r#"{"data": []}"#).is_err());
    }

    #[test]
    fn control_characters_are_removed_from_device_names() {
        let input = SAMPLE.replace(
            r#""Alias": {"type": "s", "data": "Headphones"}"#,
            r#""Alias": {"type": "s", "data": "a\u001b[31mb"}"#,
        );
        let bluetooth = parse_managed_objects(&input).expect("bluez objects");
        let headphones = bluetooth
            .devices
            .iter()
            .find(|device| device.address == "00:11:22:33:44:55")
            .expect("headphones");

        assert_eq!(headphones.name.as_deref(), Some("a[31mb"));
    }
}
