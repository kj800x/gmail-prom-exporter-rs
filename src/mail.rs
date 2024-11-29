#![allow(dead_code)]

use std::collections::HashMap;

use chrono::TimeZone;
use mailparse::{addrparse, MailAddr, MailAddrList, SingleInfo};
use serde::Deserialize;
use serde_json::Value;

use crate::auth::GoogleAuth;

#[derive(Debug, Clone, Deserialize)]
pub struct MinimalMessage {
    id: String,
    #[serde(rename = "threadId")]
    thread_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MessagesList {
    messages: Vec<MinimalMessage>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "resultSizeEstimate")]
    result_size_estimate: u64,
}

#[derive(Debug)]
pub struct UsableMessageDetails {
    pub id: String,
    pub thread_id: String,
    pub history_id: String,
    pub labels: Vec<String>,
    pub internal_date: chrono::DateTime<chrono::Utc>,
    pub from: MailAddrList,
    pub to: MailAddrList,
    pub subject: String,
}

impl UsableMessageDetails {
    pub fn as_labels(&self) -> Vec<(String, String)> {
        let mut metrics_labels = vec![];

        metrics_labels.push((
            "from".to_owned(),
            self.from.first_address().unwrap_or("unknown".to_string()),
        ));
        metrics_labels.push((
            "to".to_owned(),
            self.to.first_address().unwrap_or("unknown".to_string()),
        ));
        metrics_labels.push((
            "from_domain".to_owned(),
            self.to.first_domain().unwrap_or("unknown".to_string()),
        ));
        metrics_labels.push((
            "to_domain".to_owned(),
            self.to.first_domain().unwrap_or("unknown".to_string()),
        ));

        self.labels.iter().for_each(|label| {
            metrics_labels.push((format!("label_{}", label), "true".to_owned()));
        });

        metrics_labels
    }
}

pub trait ParseForMetrics {
    fn first_single_mailer(&self) -> Option<SingleInfo>;
    fn first_address(&self) -> Option<String>;
    fn first_domain(&self) -> Option<String>;
    fn first_display_name(&self) -> Option<String>;
}

impl ParseForMetrics for MailAddrList {
    fn first_single_mailer(&self) -> Option<SingleInfo> {
        for addr in self.iter() {
            match addr {
                MailAddr::Single(x) => return Some(x.clone()),
                _ => {}
            }
        }

        None
    }

    fn first_address(&self) -> Option<String> {
        if let Some(first) = self.first_single_mailer() {
            Some(first.addr.to_lowercase())
        } else {
            None
        }
    }

    fn first_domain(&self) -> Option<String> {
        if let Some(first) = self.first_address() {
            Some(
                first
                    .split("@")
                    .into_iter()
                    .last()
                    .unwrap()
                    .to_lowercase()
                    .to_owned(),
            )
        } else {
            None
        }
    }

    fn first_display_name(&self) -> Option<String> {
        if let Some(first) = self.first_single_mailer() {
            first.display_name
        } else {
            None
        }
    }
}

impl UsableMessageDetails {
    fn from(message: MessageDetails, labels: &HashMap<String, String>) -> Self {
        let mut from = String::new();
        let mut to = String::new();
        let mut subject = String::new();

        for header in message.payload.headers {
            match header.name.as_str() {
                "From" => from = header.value.clone(),
                "To" => to = header.value.clone(),
                "Subject" => subject = header.value.clone(),
                _ => {}
            }
        }

        let to_parsed = addrparse(&to).unwrap();
        let from_parsed = addrparse(&from).unwrap();

        Self {
            id: message.id,
            thread_id: message.thread_id,
            history_id: message.history_id,
            labels: message
                .label_ids
                .iter()
                .map(|x| labels.get(x).cloned().unwrap_or(x.clone()))
                .collect(),
            internal_date: chrono::Utc
                .timestamp_millis_opt(message.internal_date.parse().unwrap())
                .latest()
                .expect("Expected to be able to parse out a timestamp from message.internal_date"),
            from: from_parsed,
            to: to_parsed,
            subject,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MessageDetails {
    id: String,
    #[serde(rename = "threadId")]
    thread_id: String,
    #[serde(rename = "labelIds")]
    label_ids: Vec<String>,
    snippet: String,
    #[serde(rename = "historyId")]
    history_id: String,
    #[serde(rename = "internalDate")]
    internal_date: String,
    payload: MessagePart,
    #[serde(rename = "sizeEstimate")]
    size_estimate: u64,
}

#[derive(Debug, Deserialize)]
pub struct MessagePart {
    #[serde(rename = "partId")]
    part_id: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    filename: String,
    headers: Vec<MessageHeader>,
    // body: MessagePartBody,
    // parts: Vec<MessagePart>,
}

#[derive(Debug, Deserialize)]
pub struct MessageHeader {
    name: String,
    value: String,
}

// #[derive(Debug, Deserialize)]
// struct MessagePartBody {
//     size: u64,
//     data: String,
//     #[serde(rename = "attachmentId")]
//     attachment_id: String,
// }

#[derive(Debug, Deserialize)]
pub struct MessageAdded {
    message: MinimalMessage,
}

#[derive(Debug, Deserialize)]
pub struct History {
    id: String,
    #[serde(rename = "messagesAdded")]
    messages_added: Option<Vec<MessageAdded>>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryResponse {
    history: Option<Vec<History>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "historyId")]
    history_id: String,
}

pub struct MailClient {
    pub google_client: GoogleAuth,
}

impl MailClient {
    pub async fn load_labels(&mut self) -> HashMap<String, String> {
        let client = reqwest::Client::new();

        let res = loop {
            let res = client
                .get("https://www.googleapis.com/gmail/v1/users/me/labels")
                .header(
                    "Authorization",
                    format!(
                        "Bearer {}",
                        self.google_client.access_token.as_ref().unwrap()
                    ),
                )
                .send()
                .await
                .unwrap();

            let json: Value = res.json().await.unwrap();

            if GoogleAuth::needs_refresh(&json).await {
                self.google_client.do_refresh().await;
            } else {
                break json;
            }
        };

        let mut labels = HashMap::new();

        for label in res["labels"].as_array().unwrap() {
            labels.insert(
                label["id"].as_str().unwrap().to_owned(),
                label["name"].as_str().unwrap().to_owned(),
            );
        }

        labels
    }

    pub async fn fetch_mail(&mut self) -> Vec<MinimalMessage> {
        let client = reqwest::Client::new();

        let res = loop {
            let res = client
                .get("https://www.googleapis.com/gmail/v1/users/me/messages")
                .header(
                    "Authorization",
                    format!(
                        "Bearer {}",
                        self.google_client.access_token.as_ref().unwrap()
                    ),
                )
                .send()
                .await
                .unwrap();

            let json: Value = res.json().await.unwrap();

            if GoogleAuth::needs_refresh(&json).await {
                self.google_client.do_refresh().await;
            } else {
                break json;
            }
        };

        serde_json::from_value::<MessagesList>(res)
            .unwrap()
            .messages
    }

    pub async fn fetch_mail_details(
        &mut self,
        listing: Vec<MinimalMessage>,
        labels: &HashMap<String, String>,
    ) -> Vec<UsableMessageDetails> {
        let mut results = vec![];
        let client = reqwest::Client::new();

        for message in listing {
            let res = loop {
                let res = client
                    .get(&format!(
                        "https://www.googleapis.com/gmail/v1/users/me/messages/{}",
                        message.id
                    ))
                    .header(
                        "Authorization",
                        format!(
                            "Bearer {}",
                            self.google_client.access_token.as_ref().unwrap()
                        ),
                    )
                    .send()
                    .await
                    .unwrap();

                let json: Value = res.json().await.unwrap();

                if GoogleAuth::needs_refresh(&json).await {
                    self.google_client.do_refresh().await;
                } else {
                    break json;
                }
            };

            if res["error"]["code"] == 404 {
                continue;
            }

            let json: MessageDetails = serde_json::from_value(res).unwrap();
            let usable = UsableMessageDetails::from(json, &labels);

            results.push(usable);
        }

        results
    }

    pub async fn fetch_history(&mut self, starting_from: &str) -> Vec<MinimalMessage> {
        let client = reqwest::Client::new();
        let mut history_list: Vec<MinimalMessage> = vec![];
        let mut page_token: Option<String> = None;

        loop {
            let res = loop {
                let page_token_part = if page_token.is_none() {
                    "".to_string()
                } else {
                    format!("&pageToken={}", page_token.as_ref().unwrap())
                };

                let res = client
                    .get(format!(
                        "https://gmail.googleapis.com/gmail/v1/users/me/history?startHistoryId={}{}",
                        starting_from,
                        page_token_part
                    ))
                    .header(
                        "Authorization",
                        format!(
                            "Bearer {}",
                            self.google_client.access_token.as_ref().unwrap()
                        ),
                    )
                    .send()
                    .await
                    .unwrap();

                let json: Value = res.json().await.unwrap();

                if GoogleAuth::needs_refresh(&json).await {
                    self.google_client.do_refresh().await;
                } else {
                    break json;
                }
            };

            let history = serde_json::from_value::<HistoryResponse>(res).unwrap();

            if let Some(history) = history.history {
                history.into_iter().for_each(|h| {
                    if let Some(messages_added) = h.messages_added {
                        messages_added.into_iter().for_each(|m| {
                            history_list.push(m.message);
                        });
                    }
                });
            }

            if history.next_page_token.is_none() {
                break;
            } else {
                page_token = history.next_page_token;
            }
        }

        history_list
    }
}
