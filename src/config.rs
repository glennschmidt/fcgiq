use log::LevelFilter;
use serde::{Deserialize, Serialize};
use serde_yml;
use std::collections::HashMap;
use std::{fs, io, result};
use thiserror::Error;

//
// Data structures
//

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Config {
    pub fastcgi: Fastcgi,
    pub queue: Queue,
    #[serde(default)]
    pub field_mappings: FieldMappings,
    #[serde(default = "Config::default_log_level")]
    pub log_level: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Fastcgi {
    pub address: String,
    pub port: u16,
    pub script_path: String,
    pub max_parallel_requests: u32,
    #[serde(default)]
    /// A mapping of CGI environment variable names (see https://www.rfc-editor.org/rfc/rfc3875.html#section-4) to default values
    pub cgi_environment: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Queue {
    pub sqs: Sqs,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Sqs {
    #[serde(default)]
    pub api_endpoint_url: String,
    pub queue_url: String,
    pub visibility_timeout: i32,
}

pub type FieldMappings = HashMap<String, FieldMapping>;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct FieldMapping {
    pub source: FieldSource,
    /// The CGI environment variable to set (see https://www.rfc-editor.org/rfc/rfc3875.html#section-4)
    pub field: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum FieldSource {
    BodyJson,
    Metadata,
}


//
// Functions
//

impl Config {
    pub fn from_yaml_str(str: &str) -> Result<Self> {
        let config: Config = serde_yml::from_str(str)?;
        Ok(config)
    }

    pub fn from_file(path: &str) -> Result<Self> {
        let yaml = fs::read_to_string(path)?;
        Config::from_yaml_str(&yaml)
    }

    fn default_log_level() -> String {
        LevelFilter::Info.to_string()
    }
}


//
// Error handling
//

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yml::Error),
}

pub type Result<T> = result::Result<T, Error>;
