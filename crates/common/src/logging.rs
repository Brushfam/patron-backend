use tracing_core::Level;
use tracing_subscriber::{filter::Targets, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;

/// Initialize [`tracing_subscriber`] with the provided [`Config`] struct.
///
/// Besides using the provided configuration to determine the minimal log level,
/// this function also sets `sqlx` target log level to "warn" and makes log messages
/// more compact.
pub fn init(config: &Config) {
    let fmt = fmt::format().with_target(false).compact();

    let target_filters = Targets::new()
        .with_target("sqlx", Level::WARN)
        .with_target("substrate_api_client", Level::WARN)
        .with_default(config.logging.level);

    tracing_subscriber::registry()
        .with(fmt::layer().event_format(fmt))
        .with(target_filters)
        .init();
}
