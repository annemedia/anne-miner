use crate::com::api::*;
use reqwest::{Client as InnerClient, header::{HeaderMap, HeaderName}};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use url::form_urlencoded::byte_serialize;
use url::Url;

#[derive(Clone, Debug)]
pub struct Client {
    inner: InnerClient,
    account_id_to_secret_phrase: Arc<HashMap<u64, String>>,
    base_uri: Url,
    #[allow(dead_code)]  
    total_size_gb: usize,
    headers: Arc<HeaderMap>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionParameters {
    pub account_id: u64,
    pub nonce: u64,
    pub height: u64,
    pub block: u64,
    pub deadline_unadjusted: u64,
    pub deadline: u64,
    pub gen_sig: [u8; 32],
}

/// Usefull for deciding which submission parameters are the newest and best.
/// We always cache the currently best submission parameters and on fail
/// resend them with an exponential backoff. In the meantime if we get better
/// parameters the old ones need to be replaced.
impl Ord for SubmissionParameters {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.block < other.block {
            Ordering::Less
        } else if self.block > other.block {
            Ordering::Greater
        } else if self.gen_sig == other.gen_sig {
            // on the same chain, best deadline wins
            if self.deadline <= other.deadline {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        } else {
            Ordering::Less
        }
    }
}

impl PartialOrd for SubmissionParameters {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ProxyDetails {
    Enabled,
    Disabled,
}

impl Client {
    fn ua() -> String {
        format!("anne-miner/{}", env!("CARGO_PKG_VERSION"))
    }

    fn submit_nonce_headers(
        proxy_details: ProxyDetails,
        total_size_gb: usize,
        additional_headers: HashMap<String, String>,
    ) -> HeaderMap {
        let ua = Client::ua();
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", ua.to_owned().parse().unwrap());

        if proxy_details == ProxyDetails::Enabled {
            headers.insert("X-Capacity", total_size_gb.to_string().parse().unwrap());
            headers.insert("X-Miner", ua.to_owned().parse().unwrap());

            let hostname = hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
            .unwrap_or_default();
            
            headers.insert("X-Minername", hostname.parse().unwrap());
            headers.insert(
                "X-Plotfile",
                format!("anne-miner-proxy/{}", hostname).parse().unwrap(),
            );
        }

        for (key, value) in additional_headers {
            let header_name = HeaderName::from_bytes(&key.into_bytes()).unwrap();
            headers.insert(header_name, value.parse().unwrap());
        }

        headers
    }

    pub fn new(
        base_uri: Url,
        mut secret_phrases: HashMap<u64, String>,
        timeout: u64,
        total_size_gb: usize,
        proxy_details: ProxyDetails,
        additional_headers: HashMap<String, String>,
    ) -> Self {
        for secret_phrase in secret_phrases.values_mut() {
            *secret_phrase = byte_serialize(secret_phrase.as_bytes()).collect();
        }

        let headers = Client::submit_nonce_headers(proxy_details, total_size_gb, additional_headers);
        let client = InnerClient::builder()
            .timeout(Duration::from_millis(timeout))
            .build()
            .unwrap();


        Self {
            inner: client,
            account_id_to_secret_phrase: Arc::new(secret_phrases),
            base_uri,
            total_size_gb,
            headers: Arc::new(headers),
        }
    }
    pub async fn get_mining_info(&self) -> Result<MiningInfoResponse, FetchError> {
        let request = GetMiningInfoRequest {
            request_type: "getMiningInfoNew",
        };

        let mut url = self.uri_for("anne");
        url.query_pairs_mut()
            .append_pair("requestType", request.request_type);
        
        let res: reqwest::Response = self
            .inner
            .get(url)
            .headers((*self.headers).clone())
            .send()
            .await?;
        
        let bytes = res.bytes().await?;
        parse_json_getmininginfo(&bytes).map_err(FetchError::from)
    }

    pub fn uri_for(&self, path: &str) -> Url {
        let mut url = self.base_uri.clone();
        url.path_segments_mut()
            .map_err(|_| "cannot be base")
            .unwrap()
            .pop_if_empty()
            .push(path);
        url
    }


    pub async fn submit_nonce(
    &self,
    submission_data: &SubmissionParameters,
    ) -> Result<SubmitNonceResponse, FetchError> {
        let empty = "".to_owned();
        let secret_phrase = self
            .account_id_to_secret_phrase
            .get(&submission_data.account_id)
            .unwrap_or(&empty);

        let query_parts = vec![
            format!("requestType=submitNonceNew"),
            format!("accountId={}", submission_data.account_id),
            format!("nonce={}", submission_data.nonce),
            format!("blockheight={}", submission_data.height),
            format!("secretPhrase={}", secret_phrase),
            format!("deadline={}", submission_data.deadline_unadjusted),
        ];

        let query = query_parts.join("&");
        
        let mut headers = (*self.headers).clone();
        headers.insert(
            "X-Deadline",
            submission_data.deadline.to_string().parse().unwrap(),
        );

        let uri_string = self.uri_for("anne").to_string();
        let uri_string = if uri_string.contains("://localhost:") {
            uri_string.replace("://localhost:", "://127.0.0.1:")
        } else {
            uri_string
        };
        
        let mut uri = reqwest::Url::parse(&uri_string).unwrap();
        uri.set_query(Some(&query));
        

        let start = std::time::Instant::now();
        debug!("CLIENT.RS 🤔 - uri: {}", uri);
        let response = self.inner.post(uri)
        .headers(headers)
        .send()
        .await?;

        let bytes = response.bytes().await?;
        let total_time = start.elapsed();
        info!("⌛ Total request time: {:?}", total_time);
        parse_json_submitnonce(&bytes).map_err(FetchError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    static BASE_URL: &str = "http://localhost:9118/";

    #[tokio::test]
    async fn test_submit_params_cmp() {
        let submit_params_1 = SubmissionParameters {
            account_id: 7777798716640079675,
            nonce: 12,
            height: 112,
            block: 0,
            deadline_unadjusted: 7123,
            deadline: 1193,
            gen_sig: [0; 32],
        };

        let mut submit_params_2 = submit_params_1.clone();
        submit_params_2.block += 1;
        assert!(submit_params_1 < submit_params_2);

        let mut submit_params_2 = submit_params_1.clone();
        submit_params_2.deadline -= 1;
        assert!(submit_params_1 < submit_params_2);

        let mut submit_params_2 = submit_params_1.clone();
        submit_params_2.gen_sig[0] = 1;
        submit_params_2.deadline += 1;
        assert!(submit_params_1 < submit_params_2);

        let mut submit_params_2 = submit_params_1.clone();
        submit_params_2.deadline += 1;
        assert!(submit_params_1 > submit_params_2);
    }

    #[tokio::test]
    async fn test_requests() {
        let mut secret = HashMap::new();
        secret.insert(7777798716640079675, "secret".to_owned());

        let client = Client::new(
            Url::parse(BASE_URL).unwrap(),
            secret,
            5000,
            12,
            ProxyDetails::Enabled,
            HashMap::new(),
        );

        let mining_info = client
            .get_mining_info()
            .await
            .expect("Failed to fetch mining info");

        let submission = SubmissionParameters {
            account_id: 7777798716640079675,
            nonce: 12,
            height: mining_info.height,
            block: 1,
            deadline_unadjusted: 7123,
            deadline: 1193,
            gen_sig: [0; 32],
        };

        let result = client.submit_nonce(&submission).await;
        assert!(result.is_ok(), "submit_nonce failed: {:?}", result.err());
    }
}
