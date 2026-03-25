use crate::com::api::{ FetchError, MiningInfoResponse };
use crate::com::client::{ Client, ProxyDetails, SubmissionParameters };
use crate::future::prio_retry::PrioRetry;
use futures_util::stream::{ StreamExt };
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use url::Url;
use std::sync::atomic::Ordering;
use crate::miner::SUBMISSION_FAILED;
use crate::miner::SUBMISSION_SUCCESS;

use log;

#[derive(Clone)]
pub struct RequestHandler {
    client: Client,
    tx_submit_data: mpsc::UnboundedSender<SubmissionParameters>,
}

struct SubmissionLogger {
    cmsg: String,
    height: u64,
    account_id: u64,
    nonce: u64,
    deadline: u64,
    err_code: Option<i32>,
    msg: Option<String>,
    level: log::Level,
}

impl SubmissionLogger {
    fn new(cmsg: &str, height: u64, account_id: u64, nonce: u64, deadline: u64) -> Self {
        Self {
            cmsg: cmsg.to_string(),
            height,
            account_id,
            nonce,
            deadline,
            err_code: None,
            msg: None,
            level: log::Level::Error, // Default to error
        }
    }

    fn err_code(mut self, code: i32) -> Self {
        self.err_code = Some(code);
        self
    }

    fn msg(mut self, msg: &str) -> Self {
        self.msg = Some(msg.to_string());
        self
    }

    fn as_info(mut self) -> Self {
        self.level = log::Level::Info;
        self
    }

    fn as_warn(mut self) -> Self {
        self.level = log::Level::Warn;
        self
    }

    fn as_error(mut self) -> Self {
        self.level = log::Level::Error;
        self
    }
    #[allow(unused)]
    fn as_debug(mut self) -> Self {
        self.level = log::Level::Debug;
        self
    }

    fn log(self) {
        match self.level {
            log::Level::Error =>
                error!(
                    "{}, height={}, miner={}, nonce={}, deadline-legacy={}{}{}",
                    self.cmsg,
                    self.height,
                    self.account_id,
                    self.nonce,
                    self.deadline,
                    self.err_code.map(|code| format!("\n\tcode: {}", code)).unwrap_or_default(),
                    self.msg
                        .as_ref()
                        .map(|msg| format!("\n\tmessage: {}", msg))
                        .unwrap_or_default()
                ),
            log::Level::Warn =>
                warn!(
                    "{}, height={}, miner={}, nonce={}, deadline-legacy={}{}{}",
                    self.cmsg,
                    self.height,
                    self.account_id,
                    self.nonce,
                    self.deadline,
                    self.err_code.map(|code| format!("\n\tcode: {}", code)).unwrap_or_default(),
                    self.msg
                        .as_ref()
                        .map(|msg| format!("\n\tmessage: {}", msg))
                        .unwrap_or_default()
                ),
            log::Level::Info =>
                info!(
                    "{}, height={}, miner={}, nonce={}, deadline-legacy={}{}{}",
                    self.cmsg,
                    self.height,
                    self.account_id,
                    self.nonce,
                    self.deadline,
                    self.err_code.map(|code| format!("\n\tcode: {}", code)).unwrap_or_default(),
                    self.msg
                        .as_ref()
                        .map(|msg| format!("\n\tmessage: {}", msg))
                        .unwrap_or_default()
                ),
            log::Level::Debug =>
                debug!(
                    "{}, height={}, miner={}, nonce={}, deadline-legacy={}{}{}",
                    self.cmsg,
                    self.height,
                    self.account_id,
                    self.nonce,
                    self.deadline,
                    self.err_code.map(|code| format!("\n\tcode: {}", code)).unwrap_or_default(),
                    self.msg
                        .as_ref()
                        .map(|msg| format!("\n\tmessage: {}", msg))
                        .unwrap_or_default()
                ),
            log::Level::Trace =>
                log::trace!(
                    "{}, height={}, miner={}, nonce={}, deadline-legacy={}{}{}",
                    self.cmsg,
                    self.height,
                    self.account_id,
                    self.nonce,
                    self.deadline,
                    self.err_code.map(|code| format!("\n\tcode: {}", code)).unwrap_or_default(),
                    self.msg
                        .as_ref()
                        .map(|msg| format!("\n\tmessage: {}", msg))
                        .unwrap_or_default()
                ),
        }
    }
}

impl RequestHandler {
    pub fn new(
        base_uri: Url,
        secret_phrases: HashMap<u64, String>,
        timeout: u64,
        total_size_gb: usize,
        send_proxy_details: bool,
        additional_headers: HashMap<String, String>,
        handle: tokio::runtime::Handle
    ) -> RequestHandler {
        let proxy_details = if send_proxy_details {
            ProxyDetails::Enabled
        } else {
            ProxyDetails::Disabled
        };

        let client = Client::new(
            base_uri,
            secret_phrases,
            timeout,
            total_size_gb,
            proxy_details,
            additional_headers
        );

        let (tx_submit_data, rx_submit_nonce_data) = mpsc::unbounded_channel();
        RequestHandler::handle_submissions(
            client.clone(),
            rx_submit_nonce_data,
            tx_submit_data.clone(),
            handle
        );

        RequestHandler {
            client,
            tx_submit_data,
        }
    }

    fn handle_submissions(
        client: Client,
        rx: mpsc::UnboundedReceiver<SubmissionParameters>,
        tx_submit_data: mpsc::UnboundedSender<SubmissionParameters>,
        handle: tokio::runtime::Handle
    ) {
        handle.spawn(async move {
            let wrapped_rx = UnboundedReceiverStream::new(rx);
            let stream = PrioRetry::new(wrapped_rx, Duration::from_secs(3));

            let mut stream = Box::pin(stream);
            while let Some(sub_par) = stream.as_mut().next().await {
                let tx_submit_data = tx_submit_data.clone();
                let result = client.clone().submit_nonce(&sub_par).await;

                match result {
                    Ok(res) => {
                        debug!(
                            "🤔 DEBUG SUBMISSION RESPONSE: result={}, message={:?}, solution_seconds={:?}",
                            res.result,
                            res.message,
                            res.solution_seconds
                        );
                        if res.result == "success" {
                            info!(
                                "✅ SUBMISSION ACCEPTED: miner {}, nonce {}, height {}, deadline {}, seconds {}",
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.height,
                                sub_par.deadline,
                                res.solution_seconds.unwrap_or(0)
                            );
                            SUBMISSION_SUCCESS.store(true, Ordering::Relaxed);
                        } else if res.message.contains("already submit") {
                            SubmissionLogger::new(
                                "⚠️ Duplicated submission attempt - you've already submitted the same solution",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )
                                .as_warn()
                                .log();
                            SUBMISSION_SUCCESS.store(true, Ordering::Relaxed);
                            SUBMISSION_FAILED.store(false, Ordering::Relaxed);
                        } else {
                            SubmissionLogger::new(
                                "❓ Unexpected response",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )   
                                .err_code(0)
                                .msg(&res.message)
                                .as_warn()
                                .log();
                        }
                    }

                    Err(FetchError::Pool(e)) => {
                        if e.message.contains("already submit") {
                            SubmissionLogger::new(
                                "⚠️✅ SUBMISSION ACCEPTED (previously submitted)",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )
                                .as_info()
                                .log();
                            SUBMISSION_SUCCESS.store(true, Ordering::Relaxed);
                        } else if e.message == "limit exceeded" {
                            SubmissionLogger::new(
                                "🛑 ANNODE BUSY, submission limit exceeded. Waiting for the next round.",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )
                                .err_code(e.code)
                                .msg(&e.message)
                                .as_error()
                                .log();
                            SUBMISSION_FAILED.store(true, Ordering::Relaxed);
                            // let res = tx_submit_data.send(sub_par);
                            // if let Err(e) = res {
                            //     error!("can't send submission params: {}", e);
                            // }
                        } else if e.message.is_empty() {
                             SubmissionLogger::new(
                                "⚠️ ANNODE BUSY. The submission may have gone through, but no confirmation was received. Retrying… A following “duplicate submission” message indicates success.",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )
                                .as_warn()
                                .log();
                            SUBMISSION_FAILED.store(true, Ordering::Relaxed);
                            let res = tx_submit_data.send(sub_par);
                            if let Err(e) = res {
                                error!("can't send submission params: {}", e);
                            }
                        }  else {
                            SUBMISSION_FAILED.store(true, Ordering::Relaxed);
                            SubmissionLogger::new(
                                "🛑 Submission not accepted",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )
                                .err_code(e.code)
                                .msg(&e.message)
                                .as_error()
                                .log();
                        }
                    }
                    Err(FetchError::Http(x)) => {
                        SUBMISSION_FAILED.store(true, Ordering::Relaxed);
                        if x.is_timeout() {
                            SubmissionLogger::new(
                                "⚠️ Solution may have been submitted but annode didn't respond. Adjust your config.yaml timeout to 60000",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )
                                .as_warn()
                                .log();
                        } else if x.is_connect() {
                            SubmissionLogger::new(
                                "⚠️ Connection error, retrying submission",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            )
                                .as_warn()
                                .log();
                            let sub_par_clone = sub_par.clone();
                            let res = tx_submit_data.send(sub_par_clone);
                            if let Err(e) = res {
                                SubmissionLogger::new(
                                    "🛑 Can't send submission, check if annode is LIVE",
                                    sub_par.height,
                                    sub_par.account_id,
                                    sub_par.nonce,
                                    sub_par.deadline
                                )
                                    .msg(&format!("SendError: {}", e))
                                    .as_error()
                                    .log();
                            }
                        } else {
                            SubmissionLogger::new(
                                "⚠️ HTTP error, retrying submission",
                                sub_par.height,
                                sub_par.account_id,
                                sub_par.nonce,
                                sub_par.deadline
                            );
                            let sub_par_clone = sub_par.clone();
                            let res = tx_submit_data.send(sub_par_clone);
                            if let Err(e) = res {
                                SubmissionLogger::new(
                                    "🛑 Can't send submission, check if annode is LIVE",
                                    sub_par.height,
                                    sub_par.account_id,
                                    sub_par.nonce,
                                    sub_par.deadline
                                )
                                    .msg(&format!("SendError: {}", e))
                                    .as_error()
                                    .log();
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn get_mining_info<'a>(
        &'a self
    ) -> impl std::future::Future<Output = Result<MiningInfoResponse, FetchError>> + 'a {
        self.client.get_mining_info()
    }

    pub fn submit_nonce(
        &self,
        account_id: u64,
        nonce: u64,
        height: u64,
        block: u64,
        deadline_unadjusted: u64,
        deadline: u64,
        gen_sig: [u8; 32]
    ) {
        let res = self.tx_submit_data.send(SubmissionParameters {
            account_id,
            nonce,
            height,
            block,
            deadline_unadjusted,
            deadline,
            gen_sig,
        });
        if let Err(e) = res {
            error!("can't send submission params: {}", e);
        } else {
        }
    }
}

#[allow(unused)]
fn log_deadline_mismatch(
    height: u64,
    account_id: u64,
    nonce: u64,
    deadline: u64,
    deadline_audit: u64,
    solution_seconds: u64
) {
    error!(
        "submit: deadlines mismatch, height={}, miner={}, nonce={}, \
         deadline miner={}, deadline audit={}, solution seconds={}",
        height,
        account_id,
        nonce,
        deadline,
        deadline_audit,
        solution_seconds
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::runtime::Runtime;

    static BASE_URL: &str = "http://localhost:9118/";

    #[test]
    fn test_submit_nonce() {
        use url::Url;
        let rt = Runtime::new().expect("can't create runtime");
        let handle = rt.handle().clone();

        let base_url: Url = BASE_URL.parse().expect("invalid URL");

        let request_handler = RequestHandler::new(
            base_url,
            HashMap::new(),
            3,
            12,
            true,
            HashMap::new(),
            handle
        );

        request_handler.submit_nonce(7777798716640079675, 12, 111, 0, 7123, 1193, [0; 32]);
    }
}
