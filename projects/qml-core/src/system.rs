//! Explicit status input schema. Qt retains unchanged branches and signals.
use serde_json::{Map, Value};
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    if function != "patch" {
        return Err("unknown system status function".into());
    }
    let mut result = Map::new();
    if let Some(patch) = args.first().and_then(Value::as_object) {
        for (key, value) in patch {
            let valid = match key.as_str() {
                "volume" | "microphoneVolume" | "bluetoothConnected" => {
                    value.as_i64().is_some_and(|n| i32::try_from(n).is_ok())
                }
                "muted"
                | "microphoneMuted"
                | "microphoneActive"
                | "wifiEnabled"
                | "wifiAvailable"
                | "bluetoothAvailable"
                | "bluetoothPowered"
                | "bluetoothScanning"
                | "bluetoothReceiver"
                | "bluetoothDiscoverable"
                | "cameraActive"
                | "screenRecording"
                | "airpodsEarDetection"
                | "dnd" => value.is_boolean(),
                "connection" | "connectionType" | "connectivity" | "ipAddress" | "gateway"
                | "networkInterface" | "voxtypeStatus" | "cameraDevice" => {
                    value.as_str().is_some_and(|text| text.len() <= 4096)
                }
                "networkAddresses" | "bluetoothDevices" | "cameraDevices" | "audioDevices"
                | "batteries" | "trayHidden" => {
                    value.as_array().is_some_and(|items| items.len() <= 16384)
                }
                "tailscale" | "protonVpn" | "sshServer" | "headphones" | "barModules"
                | "agentStates" | "notifications" => value.is_object(),
                _ => false,
            };
            if valid {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(Value::Object(result))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn status_never_writes_qobject_metadata_or_coerces_invalid_types() {
        assert_eq!(call("patch",&[json!({"objectName":"changed","parent":{},"__proto__":{},"volume":"12","muted":[],"dnd":true,"headphones":null,"audioDevices":[],"volumeChanged":"x"})]).unwrap(),json!({"dnd":true,"audioDevices":[]}));
        assert_eq!(
            call(
                "patch",
                &[json!({"volume":12,"connection":"Wi-Fi","headphones":{"connected":true}})]
            )
            .unwrap(),
            json!({"volume":12,"connection":"Wi-Fi","headphones":{"connected":true}})
        );
    }
}
