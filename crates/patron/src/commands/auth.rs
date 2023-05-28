use std::time::Duration;

use derive_more::{Display, Error, From};
use indicatif::ProgressBar;
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::config::{
    default_server_path, default_web_path, AuthenticationConfig, AuthenticationConfigError,
};

const EXCHANGE_TOKEN_LENGTH: usize = 64;

#[derive(Serialize)]
struct ExchangeRequest<'a> {
    cli_token: &'a str,
}

#[derive(Deserialize)]
struct ExchangeResponse {
    token: String,
}

#[derive(Debug, Display, From, Error)]
pub(crate) enum AuthError {
    Authentication(AuthenticationConfigError),
    Http(reqwest::Error),
}

pub(crate) fn auth(server_path: Option<String>, web_path: Option<String>) -> Result<(), AuthError> {
    let server_domain = server_path.unwrap_or(default_server_path());
    let web_domain = web_path.unwrap_or(default_web_path());

    let cli_token = Alphanumeric.sample_string(&mut thread_rng(), EXCHANGE_TOKEN_LENGTH);

    let exchange_url = format!("{web_domain}/login?cli_token={cli_token}");

    let pg = ProgressBar::new_spinner();

    pg.enable_steady_tick(Duration::from_millis(150));
    pg.println(format!("Opening {exchange_url}"));

    let _ = open::that_in_background(&exchange_url);

    loop {
        pg.set_message("Awaiting for authentication token...");

        let build_session_status = reqwest::blocking::Client::new()
            .post(format!("{server_domain}/auth/exchange"))
            .json(&ExchangeRequest {
                cli_token: &cli_token,
            })
            .send()?
            .error_for_status();

        match build_session_status {
            Ok(response) => {
                AuthenticationConfig::write_token(
                    response.json::<ExchangeResponse>()?.token,
                    server_domain,
                    web_domain,
                )?;
                break;
            }
            Err(error) if error.status() == Some(StatusCode::NOT_FOUND) => {}
            Err(error) => Err(error)?,
        };

        std::thread::sleep(Duration::from_secs(3));
    }

    pg.finish_with_message("Authentication completed.");

    Ok(())
}
