use serde_json::Value;
use std::collections::HashMap;

/// Represents a queue item.
pub struct Item {
    pub id: String,
    pub data: Vec<u8>,
    pub metadata: HashMap<String, String>,
}

impl Item {
    /// Attempt to parse the item's content as JSON data.
    pub fn parse_data_as_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_slice(&self.data)
    }

    /// Treat the item's content as a JSON object, and retrieve the value of the given key from the
    /// object (if it's a string)
    pub fn get_string_from_data_json_object(&self, key: &str) -> Option<String> {
        let json = self.parse_data_as_json();
        if json.is_err() {
            log::debug!("[task {}] unable to parse queue item body as JSON: {:?}", self.id, json.unwrap_err());
            return None;
        }
        Some(json.unwrap().get(key)?.as_str()?.to_string())
    }
}
