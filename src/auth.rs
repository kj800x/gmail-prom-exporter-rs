use std::collections::HashMap;

use serde_json::Value;
use url::{self, Url};

#[derive(Debug)]
pub struct GoogleAuth {
    client_id: String,
    client_secret: String,
    pub access_token: Option<String>,
    refresh_token: Option<String>,
}

impl GoogleAuth {
    pub fn new_from_env() -> Self {
        Self {
            client_id: std::env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set"),
            client_secret: std::env::var("GOOGLE_CLIENT_SECRET")
                .expect("GOOGLE_CLIENT_SECRET must be set"),
            access_token: std::env::var_os("GOOGLE_ACCESS_TOKEN")
                .map(|s| s.to_string_lossy().to_string()),
            refresh_token: std::env::var_os("GOOGLE_REFRESH_TOKEN")
                .map(|s| s.to_string_lossy().to_string()),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.access_token.is_some()
    }

    pub fn get_auth_url(&self) -> String {
        let mut params: HashMap<&str, String> = HashMap::new();
        params.insert("client_id", self.client_id.clone());
        params.insert("redirect_uri", "http://127.0.0.1:8080".to_owned());
        params.insert(
            "scope",
            "https://www.googleapis.com/auth/gmail.readonly".to_owned(),
        );
        params.insert("access_type", "offline".to_owned());
        params.insert("response_type", "code".to_owned());

        Url::parse_with_params("https://accounts.google.com/o/oauth2/v2/auth", params)
            .unwrap()
            .to_string()
    }

    // https://example.com/?code=4/0AeanS0Z4DfVlyGt7uyKggf1Kfm_FCuBrJjSNebnyqSwwU9e2Az_PY79HZ1XsYkF8mmjv3Q&scope=https://www.googleapis.com/auth/gmail.readonly
    pub async fn handle_callback_url(&mut self, callback_url: String) {
        let url = Url::parse(&callback_url).unwrap();
        let code = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .expect("expected callback url to have 'code' query param")
            .1;

        let client = reqwest::Client::new();
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code.as_ref()),
                ("client_id", self.client_id.as_ref()),
                ("client_secret", self.client_secret.as_ref()),
                ("redirect_uri", "http://127.0.0.1:8080"),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .unwrap();

        let response_json: serde_json::Value = response
            .json()
            .await
            .expect("expected token exchange to return json");

        println!("response_json: {:?}", response_json);

        self.access_token = Some(
            response_json["access_token"]
                .as_str()
                .expect("expected token exchange response to include an access_token")
                .to_owned(),
        );
        self.refresh_token = Some(
            response_json["refresh_token"]
                .as_str()
                .expect("expected token exchange response to include a refresh_token")
                .to_owned(),
        );
    }

    pub async fn do_refresh(&mut self) {
        let client = reqwest::Client::new();

        println!("Refresh required, refreshing...");

        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                (
                    "refresh_token",
                    &self
                        .refresh_token
                        .clone()
                        .expect("refresh token required during potential_refresh"),
                ),
                ("grant_type", &"refresh_token".to_string()),
            ])
            .send()
            .await
            .unwrap();

        let response_json: serde_json::Value = response
            .json()
            .await
            .expect("expected token exchange to return json");

        println!("refresh response_json: {:?}", response_json);

        self.access_token = Some(
            response_json["access_token"]
                .as_str()
                .expect("expected token exchange response to include an access_token")
                .to_owned(),
        );

        println!(
            "!IMPORTANT! Access token refreshed, update env vars: {}",
            self.access_token.as_ref().unwrap()
        );
    }

    pub async fn needs_refresh(json: &Value) -> bool {
        if json["error"]["code"] == 401 {
            true
        } else {
            false
        }
    }
}
