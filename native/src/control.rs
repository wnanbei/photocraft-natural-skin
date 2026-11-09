//! Authenticated loopback client. Never automatically retry a mutation after a
//! timeout: the editor may still be executing it.
use crate::Result;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::Duration,
};

pub struct ControlClient {
    stream: BufReader<TcpStream>,
    sequence: u64,
}

/// Conservative MVP source guard. This detects stale state, but it is not an
/// atomic compare-and-apply operation on the editor; automatic replacement must
/// not claim to be race-free with the current index-based host API.
pub struct SourceTicket {
    pub document_index: usize,
    snapshot: Value,
}
impl SourceTicket {
    pub fn from_snapshot(document_index: usize, snapshot: Value) -> Result<Self> {
        if snapshot["mode"] != "Rgb"
            || snapshot["hasSelection"] != false
            || snapshot["quickMask"] != false
            || snapshot["channels"]["target"]["kind"] != "composite"
        {
            return Err("Native MVP needs an RGB layer with no selection/channel target".into());
        }
        let layer = snapshot["activeLayer"].as_u64().ok_or("No active layer")?;
        if snapshot["selectedLayers"] != json!([layer]) {
            return Err("Select one pixel layer".into());
        }
        let active = snapshot["layers"]
            .as_array()
            .and_then(|items| items.iter().find(|v| v["id"].as_u64() == Some(layer)))
            .ok_or("Active layer is not top-level")?;
        if active["kind"] != "Pixel"
            || active["blend"] != "Normal"
            || active["opacity"].as_f64() != Some(1.0)
            || active["fill"].as_f64() != Some(1.0)
            || active["hasMask"] != false
            || active["hasVectorMask"] != false
            || active["clipped"] != false
            || active["effects"]["enabled"] == true
        {
            return Err("Native MVP requires an unmasked, unclipped Normal pixel layer at 100% without effects".into());
        }
        if snapshot["revision"].as_u64().is_none() {
            return Err("Missing document revision".into());
        }
        Ok(Self {
            document_index,
            snapshot,
        })
    }
    pub fn matches(&self, current: &Value) -> bool {
        [
            "revision",
            "activeLayer",
            "width",
            "height",
            "mode",
            "depth",
            "name",
            "path",
            "hasSelection",
            "quickMask",
            "channels",
            "selectedLayers",
        ]
        .iter()
        .all(|key| self.snapshot[*key] == current[*key])
    }
    pub fn check(&self, client: &mut ControlClient) -> Result<()> {
        let current =
            client.command("document.inspect", json!({"document":self.document_index}))?;
        if self.matches(&current) {
            Ok(())
        } else {
            Err("Source changed; do not apply this result".into())
        }
    }
}
impl ControlClient {
    pub fn connect(address: SocketAddr, token_file: &Path) -> Result<Self> {
        if !address.ip().is_loopback() {
            return Err("Control connection must remain on loopback".into());
        }
        let token = fs::read_to_string(token_file)?;
        let token = token.trim();
        if token.len() != 64 || !token.bytes().all(|x| x.is_ascii_hexdigit()) {
            return Err("Invalid control token file".into());
        }
        let socket = TcpStream::connect_timeout(&address, Duration::from_secs(3))?;
        socket.set_read_timeout(Some(Duration::from_secs(30)))?;
        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        let mut client = Self {
            stream: BufReader::new(socket),
            sequence: 0,
        };
        client.call("auth", json!({"token":token}))?;
        Ok(client)
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Request sequence exhausted")?;
        let mut request =
            serde_json::to_vec(&json!({"id":self.sequence,"method":method,"params":params}))?;
        if request.len() > 8 * 1024 * 1024 {
            return Err("Control request is too large".into());
        }
        request.push(b'\n');
        self.stream.get_mut().write_all(&request)?;
        self.stream.get_mut().flush()?;
        let mut bytes = Vec::new();
        (&mut self.stream)
            .take(8 * 1024 * 1024)
            .read_until(b'\n', &mut bytes)?;
        if bytes.last() != Some(&b'\n') {
            return Err("Control response ended or exceeded the size limit; do not retry a mutation automatically".into());
        }
        let mut reply: Value = serde_json::from_slice(&bytes)?;
        if reply["id"].as_u64() != Some(self.sequence) {
            return Err("Unexpected control response id".into());
        }
        if reply["ok"] != true {
            return Err(reply["error"]
                .as_str()
                .unwrap_or("Control request failed")
                .to_string()
                .into());
        }
        Ok(reply["result"].take())
    }
    pub fn command(&mut self, command: &str, params: Value) -> Result<Value> {
        self.call("engine.execute", json!({"command":command,"params":params}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_nonlocal_endpoints_before_reading_credentials() {
        assert!(
            ControlClient::connect(
                "192.0.2.1:1".parse().unwrap(),
                Path::new("not-a-token-file")
            )
            .err()
            .unwrap()
            .to_string()
            .contains("loopback")
        );
    }
    #[test]
    fn source_ticket_rejects_stale_revisions_and_unsupported_targets() {
        let snapshot = json!({"mode":"Rgb","hasSelection":false,"quickMask":false,"channels":{"target":{"kind":"composite"}},"activeLayer":4,"selectedLayers":[4],"revision":9,
            "layers":[{"id":4,"kind":"Pixel","blend":"Normal","opacity":1.0,"fill":1.0,"hasMask":false,"hasVectorMask":false,"clipped":false}]});
        let ticket = SourceTicket::from_snapshot(0, snapshot.clone()).unwrap();
        assert!(ticket.matches(&snapshot));
        let mut changed = snapshot.clone();
        changed["revision"] = json!(10);
        assert!(!ticket.matches(&changed));
        changed = snapshot.clone();
        changed["activeLayer"] = json!(5);
        assert!(!ticket.matches(&changed));
        changed = snapshot;
        changed["hasSelection"] = json!(true);
        assert!(SourceTicket::from_snapshot(0, changed).is_err());
    }
}
