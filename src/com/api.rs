use bytes::Bytes;
use serde::de::{self, DeserializeOwned};
use std::fmt;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(unused)]
pub struct SubmitNonceRequest<'a> {
    pub request_type: &'a str,
    pub account_id: u64,
    pub nonce: u64,
    pub secret_phrase: Option<&'a String>,
    pub blockheight: u64,
    pub deadline: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMiningInfoRequest<'a> {
    pub request_type: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitNonceResponse {
    #[allow(unused)]
    pub result: String,
    #[allow(unused)]
    pub message: String,
    pub solution_seconds: Option<u64>, 
}

#[derive(Deserialize)]
pub struct MiningInfoResponse {
    #[serde(rename = "generationSignature")]
    pub generation_signature: String,

    #[serde(deserialize_with = "from_str_or_int")]
    pub height: u64,

    #[serde(
        default = "default_target_deadline",
        deserialize_with = "from_str_or_int"
    )]
    pub target_deadline: u64,

    pub annode_mode: String,
    pub amp: String, 
    pub share_mining_ok: bool,
    
    #[allow(unused)]
    pub tminus: u64,
    
    #[allow(unused)]
    pub alerts: String,
    
    #[allow(unused)]
    pub debug: String,

    #[serde(flatten)]
    #[allow(unused)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

fn default_target_deadline() -> u64 {
    std::u64::MAX
}
#[allow(unused)]
fn default_annode_mode() -> String {
    "SYNC".to_string()
}
#[allow(unused)]
fn default_amp() -> String {
    "NOT MINING".to_string()
}
#[allow(unused)]
fn default_share_mining_ok() -> bool {
    false
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoolErrorWrapper {
    error: PoolError,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug)]
pub enum FetchError {
    Http(reqwest::Error),
    Pool(PoolError),
}

impl From<reqwest::Error> for FetchError {
    fn from(err: reqwest::Error) -> FetchError {
        FetchError::Http(err)
    }
}

impl From<PoolError> for FetchError {
    fn from(err: PoolError) -> FetchError {
        FetchError::Pool(err)
    }
}

fn from_str_or_int<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct StringOrIntVisitor;

    impl<'de> de::Visitor<'de> for StringOrIntVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or int")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            v.parse::<u64>().map_err(de::Error::custom)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
            Ok(v)
        }
    }

    deserializer.deserialize_any(StringOrIntVisitor)
}

pub fn parse_json_submitnonce<T: DeserializeOwned>(body: &Bytes) -> Result<T, PoolError> {
     
    let raw_response = String::from_utf8_lossy(body.as_ref());
    use std::cell::RefCell;
    thread_local! {
        static PREV_STATUS: RefCell<(String, String)> = RefCell::new(("".to_string(), "".to_string()));
    }


    #[cfg(windows)]
    mod display {
        pub fn format_submit_result(result: &str, message: &str, solution_seconds: u64) -> String {
            if result == "success" {
                format!("submitNonce: SUCCESS | solution seconds: {}", solution_seconds)
            } 
            else {
                format!("submitNonce: FAILED | {}", message)
            }
        }
    }

    #[cfg(not(windows))]
    mod display {
        pub fn format_submit_result(result: &str, message: &str, solution_seconds: u64) -> String {
            if result == "success" {
                format!("submitNonce: ✅ SUCCESS | solution seconds: {}", solution_seconds)
            } else {
                format!("submitNonce: ❌ FAILED | {}", message)
            }
        }
    }
    
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw_response) {
        if let Some(result_value) = parsed.get("result") {
            if let Some(result_str) = result_value.as_str() {
                if result_str == "success" {
                    let message = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
                    let solution_seconds = parsed.get("solution_seconds").and_then(|v| v.as_u64()).unwrap_or(0);
                    let _result_line = display::format_submit_result("success", message, solution_seconds);
                } 
                else {
                    let message = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
                    let _result_line = display::format_submit_result(result_str, message, 0);
                }
            }
        }
    }
        
    match serde_json::from_slice(body.as_ref()) {
        Ok(x) => Ok(x),
        Err(e) => {
           debug!("JSON parsing error: {}", e);
           let preview_len = 1000.min(raw_response.len());
           debug!("Full response ({} bytes, first {} chars): {}", 
                  body.len(), preview_len, &raw_response[..preview_len]);
           
           debug!("First 50 bytes hex: {:02x?}", &body.as_ref()[..body.len().min(50)]);
 
            match serde_json::from_slice::<PoolErrorWrapper>(body.as_ref()) {
                Ok(x) => Err(x.error),
                _ => {
                    let v = body.to_vec();
                    Err(PoolError {
                        code: 0,
                        message: String::from_utf8_lossy(&v).to_string(),
                    })
                }
            }
        }
    }
}

pub fn parse_json_getmininginfo<T: DeserializeOwned>(body: &Bytes) -> Result<T, PoolError> {
    let raw_response = String::from_utf8_lossy(body.as_ref());
    
    use std::cell::RefCell;
    thread_local! {
        static PREV_STATUS: RefCell<(String, String)> = RefCell::new(("".to_string(), "".to_string()));
    }

    #[cfg(windows)]
    mod display {
        pub fn format_mining_status(annode_mode: &str, amp: &str, height: &str) -> String {
            format!("MINING INFO: {} | {} | height={}", annode_mode, amp, height)
        }
    }

    #[cfg(not(windows))]
    mod display {
        pub fn format_mining_status(annode_mode: &str, amp: &str, height: &str) -> String {
            let annode_icon = if annode_mode == "LIVE" { "🟢" } else { "🔴" };
            let amp_icon = if amp == "MINING" { "🟢" } else { "🔴" };
            format!("MINING INFO: {} {} | {} {} | height={}", 
                    annode_icon, annode_mode, amp_icon, amp, height)
        }
    }
    
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw_response) {
        if parsed.get("annode_mode").is_some() || parsed.get("height").is_some() {
            let annode_mode = parsed.get("annode_mode").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
            let amp = parsed.get("amp").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
            let height = parsed.get("height").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
            #[allow(unused)]
            let share_mining_ok = parsed.get("share_mining_ok").and_then(|v| v.as_bool()).unwrap_or(false);
            let tminus = parsed.get("tminus").and_then(|v| v.as_u64()).unwrap_or(0);
            let should_print_tminus = tminus > 0 && tminus != 9999 && (tminus % 5 == 0);
            let alerts = parsed.get("alerts").and_then(|v| v.as_str()).unwrap_or("");
            let debug = parsed.get("debug").and_then(|v| v.as_str()).unwrap_or("");
            
            if alerts != "" {
                info!("ALERT!! {}", alerts);
            }
            if debug != "" {
                info!("{}", debug);
            }
            if should_print_tminus {
                debug!("estimated grace mode in tminus={} secs", tminus);
            } 
            
            let should_log = PREV_STATUS.with(|prev| {
                let mut prev = prev.borrow_mut();
                let has_changed = prev.0 != annode_mode || prev.1 != amp;
                if has_changed {
                    *prev = (annode_mode.to_string(), amp.to_string());
                }
                has_changed
            });
            
            if should_log || annode_mode == "UNKNOWN" || amp == "UNKNOWN" || height == "UNKNOWN" {
                if annode_mode == "UNKNOWN" || amp == "UNKNOWN" || height == "UNKNOWN" {
                    info!("RAW API RESPONSE: {}", raw_response);
                }
                
                let status_line = display::format_mining_status(annode_mode, amp, height);
                info!("{}", status_line);
            }
        } 

        else if raw_response != "{\"result\":\"success\"}" && !raw_response.trim().is_empty() {
            info!("API Response: {}", raw_response);
        }

    } else {
        error!("JSON PARSE FAILED - Raw response: {}", raw_response);
    }
        
    match serde_json::from_slice(body.as_ref()) {
        Ok(x) => Ok(x),
        Err(e) => {
            println!("JSON parsing error: {}", e);
            match serde_json::from_slice::<PoolErrorWrapper>(body.as_ref()) {
                Ok(x) => Err(x.error),
                _ => {
                    let v = body.to_vec();
                    Err(PoolError {
                        code: 0,
                        message: String::from_utf8_lossy(&v).to_string(),
                    })
                }
            }
        }
    }
}