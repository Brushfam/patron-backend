use std::sync::Arc;

use db::{log, ActiveModelTrait, DatabaseConnection};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::error;

/// A single log entry passed from the build session process.
pub(crate) struct LogEntry {
    /// Related build session identifier.
    pub(crate) build_session_id: i64,

    /// Log entry text.
    ///
    /// Be aware, that there is no guarantee that this text
    /// contains only a single line of logs.
    pub(crate) text: String,
}

/// Start log collection process.
///
/// [`Future`] returned from this function should be
/// spawned as a background process.
///
/// [`Future`]: std::future::Future
pub(crate) async fn collect_logs(
    db: Arc<DatabaseConnection>,
    mut receiver: UnboundedReceiver<LogEntry>,
) {
    while let Some(log_entry) = receiver.recv().await {
        let insert = log::ActiveModel {
            build_session_id: db::ActiveValue::Set(log_entry.build_session_id),
            text: db::ActiveValue::Set(log_entry.text),
            ..Default::default()
        }
        .insert(&*db)
        .await;

        if let Err(e) = insert {
            error!(%e, "unable to insert log entry")
        }
    }
}
