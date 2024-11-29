use crate::auth::GoogleAuth;
mod auth;
mod mail;
use chrono::Duration;
use clap::{Parser, Subcommand};
use metrics::{counter, describe_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Backfill {
        // #[arg(long)]
        // victoria_metrics_endpoint: String,

        // #[arg(long)]
        // start_ts: i64,

        // #[arg(long)]
        // end_ts: Option<i64>,
    },
    FetchRecentMail {
        #[arg(long)]
        starting_from: String,

        #[arg(long)]
        sleep_interval: u64,
    },
}

#[::tokio::main]
async fn main() {
    let mut google_auth = GoogleAuth::new_from_env();

    println!("immediately after starting {:?}", google_auth);

    if let Some(callback_code) = std::env::var_os("GOOGLE_CALLBACK") {
        println!("handling callback url...");
        let callback_code = callback_code.to_string_lossy().to_string();
        google_auth.handle_callback_url(callback_code).await;
        println!("after handling callback url {:?}", google_auth);
    }

    if google_auth.is_authenticated() {
        println!("Authenticated!");
    } else {
        println!("Not authenticated!");

        let auth_url = google_auth.get_auth_url();
        println!("Auth URL: {}", auth_url);

        println!("Please visit the URL above to authenticate.");
        println!("Set the GOOGLE_CALLBACK environment variable to the code you receive.");

        std::process::exit(1);
    }

    let mut mail = mail::MailClient {
        google_client: google_auth,
    };

    let cli = Cli::parse();

    match cli.command {
        Commands::Backfill {
            // victoria_metrics_endpoint,
            // start_ts,
            // end_ts,
        } => {
            println!("backfilling...");
            let labels = mail.load_labels().await;
            let mail_listing = mail.fetch_mail().await;
            let mail_details = mail.fetch_mail_details(mail_listing, &labels).await;

            if let Some(message) = mail_details.first() {
                println!("Latest message history id: {}", message.history_id);
            }
        }
        Commands::FetchRecentMail {
            starting_from: initial_starting_from,
            sleep_interval,
        } => {
            let mut starting_from = initial_starting_from.clone();
            let labels = mail.load_labels().await;

            PrometheusBuilder::new()
                .idle_timeout(
                    MetricKindMask::ALL,
                    Some(
                        Duration::seconds((sleep_interval + (60 * 5)) as i64)
                            .to_std()
                            .unwrap(),
                    ),
                )
                .with_http_listener(([0, 0, 0, 0], 9090))
                .install()
                .expect("Failed to install Prometheus recorder");

            describe_counter!(
                "email_received",
                "A counter for every email received."
            );

            loop {
                println!("Fetching recent mail...");
                let history = mail.fetch_history(&starting_from).await;
                let mail_details = mail.fetch_mail_details(history, &labels).await;

                if !mail_details.is_empty() {
                    println!("Found more mail: {}", mail_details.len());
                    println!("{:#?}", mail_details);
                    starting_from = mail_details.last().unwrap().history_id.clone();

                    for message in mail_details {
                        counter!(
                            "email_received",
                            1,
                            &message.as_labels()
                        );
                    }
                } else {
                    println!("No new mail found.");
                }

                // Sleep
                let sleep_duration = std::time::Duration::from_secs(sleep_interval);
                println!("Sleeping...");
                std::thread::sleep(sleep_duration);
            }
        }
    }
}
