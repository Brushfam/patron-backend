use tracing_core::Level;
use tracing_subscriber::{filter::Targets, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;

pub fn init(config: &Config) {
    let fmt = fmt::format().with_target(false).compact();

    let target_filters = Targets::new()
        .with_target("sqlx", Level::WARN)
        .with_default(config.logging.level);

    tracing_subscriber::registry()
        .with(fmt::layer().event_format(fmt))
        .with(target_filters)
        .init();
}
