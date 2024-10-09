use fastcgi_client::{Client, Params, Request};
use std::collections::HashMap;
use std::result;
use thiserror::Error;
use tokio::{io, net::TcpStream};

//
// Data structures
//

/// Abstraction for a FastCGI worker pool.
pub struct Pool {
    address: String,
    port: u16,
    script_path: String,
    cgi_environment: HashMap<String, String>,
}

/// Holds the output from an execution of a FastCGI script.
pub struct ScriptOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub type HttpResponse = http::Response<Vec<u8>>;


//
// Functions
//

impl Pool {
    pub fn new(address: String, port: u16, script_path: String, cgi_environment: HashMap<String, String>) -> Pool {
        Pool { address, port, script_path, cgi_environment }
    }

    async fn connect(&self) -> Result<TcpStream> {
        Ok(TcpStream::connect((self.address.clone(), self.port)).await?)
    }

    pub async fn dispatch(&self, stdin: &[u8], environment_overrides: HashMap<String, String>) -> Result<ScriptOutput> {
        let client = Client::new(self.connect().await?);

        //Set fallback defaults for essential CGI environment fields
        let mut params = Params::default()
            .content_length(stdin.len())
            .query_string("")
            .remote_addr("127.0.0.1")
            .request_method("POST")
            .script_filename(&self.script_path)
            .script_name("/")
            .server_name("localhost")
            .server_port(443)
            .server_software("fcgiq");

        //Override CGI environment fields from the config file
        for (key, val) in self.cgi_environment.iter() {
            params.insert(key.into(), val.into());
        }

        //Override CGI environment fields from the task request
        for (key, val) in environment_overrides.iter() {
            params.insert(key.into(), val.into());
        }

        if log::max_level() >= log::Level::Debug {
            let env_debug = params.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<String>>()
                .join(", ");
            log::debug!("Dispatching request with CGI environment {}", env_debug);
        }

        let request = Request::new(params, stdin);
        let response = client.execute_once(request).await?;
        let stdout = response.stdout
            .ok_or(Error::HttpResponse(String::from("empty response")))?;
        let stderr = response.stderr.unwrap_or_default();

        Ok(ScriptOutput { stdout, stderr })
    }
}

/// Parse a CGI response.
///
/// This is basically an HTTP response without the HTTP status line. The desired status code is
/// instead communicated in a `Status:` header.
///
/// See: https://www.rfc-editor.org/rfc/rfc3875.html#section-6
fn parse_cgi_response(bytes: &[u8]) -> Result<HttpResponse> {
    // Use a dummy status line to simulate a regular HTTP response, so we can use an HTTP response parser.
    let mut buffer: Vec<u8> = Vec::from("HTTP/1.1 200 OK\n");
    buffer.extend_from_slice(bytes);

    let mut header_buffer = [httparse::EMPTY_HEADER; 64];
    let mut parsed_response = httparse::Response::new(&mut header_buffer);

    match parsed_response.parse(buffer.as_slice())? {
        httparse::Status::Partial => Err(Error::HttpResponse(String::from("incomplete HTTP response"))),
        httparse::Status::Complete(header_size) => {
            let mut builder = http::Response::builder();
            for header in parsed_response.headers {
                if header.name == "Status" {
                    let value_str = String::from_utf8_lossy(header.value);
                    let status: u16 = value_str[0..3].parse()?;
                    builder = builder.status(status);
                }
                builder = builder.header(header.name, header.value);
            }
            Ok(builder.body(buffer[header_size..].to_vec())?)
        },
    }
}

impl ScriptOutput {
    /// Interpret the contents of stdout as a UTF-8 string, if possible
    pub fn stdout_string(&self) -> Option<String> {
        String::from_utf8(self.stdout.clone()).ok()
    }

    /// Interpret the contents of stderr as a UTF-8 string, if possible
    pub fn stderr_string(&self) -> Option<String> {
        String::from_utf8(self.stderr.clone()).ok()
    }
}

impl TryFrom<ScriptOutput> for HttpResponse {
    type Error = Error;

    fn try_from(value: ScriptOutput) -> result::Result<Self, Self::Error> {
        parse_cgi_response(value.stdout.as_slice())
    }
}


//
// Error handling
//

#[derive(Debug, Error)]
pub enum Error {
    #[error("error connecting to FastCGI")]
    Io(#[from] io::Error),

    #[error("FastCGI error")]
    FastCgi(#[from] fastcgi_client::ClientError),

    #[error("invalid HTTP response: {0}")]
    HttpResponse(String),
}

impl From<httparse::Error> for Error {
    fn from(value: httparse::Error) -> Self {
        Error::HttpResponse(value.to_string())
    }
}

impl From<http::Error> for Error {
    fn from(value: http::Error) -> Self {
        Error::HttpResponse(value.to_string())
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(_value: std::num::ParseIntError) -> Self {
        Error::HttpResponse("invalid status code".to_string())
    }
}

pub type Result<T> = result::Result<T, Error>;
