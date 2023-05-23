use std::sync::Arc;

use db::{log, ActiveModelTrait, DatabaseConnection};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::error;

pub(crate) struct LogEntry {
    pub(crate) build_session_id: i64,
    pub(crate) text: String,
}

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
