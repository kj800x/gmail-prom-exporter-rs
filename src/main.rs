use crate::auth::GoogleAuth;
mod auth;
mod mail;
use chrono::Duration;
use clap::{Parser, Subcommand};
use metrics::{counter, describe_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;
use uuid::Uuid;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    FetchLatestMessageId {
        // #[arg(long)]
        // victoria_metrics_endpoint: String,

        // #[arg(long)]
        // start_ts: i64,

        // #[arg(long)]
        // end_ts: Option<i64>,
    },
    WatchInbox {
        #[arg(long)]
        starting_from: String,

        #[arg(long)]
        sleep_interval: u64,
    },
}

#[::tokio::main]
async fn main() {
    let google_auth = GoogleAuth::load_from_env().await;
    let mut mail = mail::MailClient {
        google_client: google_auth,
    };

    let cli = Cli::parse();

    match cli.command {
        Commands::FetchLatestMessageId {
            // victoria_metrics_endpoint,
            // start_ts,
            // end_ts,
        } => {
            println!("fetching latest message id...");
            let labels = mail.load_labels().await;
            let mail_listing = mail.fetch_mail().await;
            let mail_details = mail.fetch_mail_details(mail_listing, &labels).await;

            if let Some(message) = mail_details.first() {
                println!("Latest message history id: {}", message.history_id);
            }
        }
        Commands::WatchInbox {
            starting_from: initial_starting_from,
            sleep_interval,
        } => {
            let mut starting_from = initial_starting_from.clone();
            let labels = mail.load_labels().await;

            PrometheusBuilder::new()
                .idle_timeout(
                    MetricKindMask::ALL,
                    Some(
                        Duration::days(365)
                            .to_std()
                            .unwrap(),
                    ),
                )
                .add_global_label("instance_id", Uuid::new_v4())
                .with_http_listener(([0, 0, 0, 0], 9090))
                .install()
                .expect("Failed to install Prometheus recorder");

            describe_counter!(
                "email_received",
                "A counter for every email received."
            );
            describe_counter!(
                "email_polls",
                "A counter for every time we checked for emails."
            );

            println!("Beginning silent watch for new mail...");

            loop {
                let history = mail.fetch_history(&starting_from).await;
                let mail_details = mail.fetch_mail_details(history, &labels).await;
                counter!("email_polls", 1);

                if !mail_details.is_empty() {
                    println!("Found more mail: {} messages", mail_details.len());
                    // println!("{:#?}", mail_details);
                    starting_from = mail_details.last().unwrap().history_id.clone();

                    for message in mail_details {
                        counter!(
                            "email_received",
                            1,
                            &message.as_labels()
                        );
                    }
                }

                // Sleep
                let sleep_duration = std::time::Duration::from_secs(sleep_interval);
                std::thread::sleep(sleep_duration);
            }
        }
    }
}
