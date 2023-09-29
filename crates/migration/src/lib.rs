pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_users_table;
mod m20220101_000002_create_public_keys_table;
mod m20220101_000003_create_authentication_tokens_table;
mod m20220101_000004_create_nodes_table;
mod m20220101_000005_create_codes_table;
mod m20220101_000006_create_contracts_table;
mod m20220101_000007_create_source_codes_table;
mod m20220101_000008_create_files_table;
mod m20220101_000009_create_build_sessions_table;
mod m20220101_000010_create_build_session_tokens_table;
mod m20220101_000011_create_logs_table;
mod m20220101_000012_create_cli_tokens_table;
mod m20220101_000013_create_events_table;
mod m20220101_000014_remove_node_schema;
mod m20220101_000015_remove_rust_version;
mod m20220101_000016_add_project_directory;
mod m20220101_000017_create_diagnostics_table;

pub(crate) use m20220101_000001_create_users_table::Users;
pub(crate) use m20220101_000003_create_authentication_tokens_table::AuthenticationTokens;
pub(crate) use m20220101_000004_create_nodes_table::Nodes;
pub(crate) use m20220101_000007_create_source_codes_table::SourceCodes;
pub(crate) use m20220101_000008_create_files_table::Files;
pub(crate) use m20220101_000009_create_build_sessions_table::BuildSessions;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_users_table::Migration),
            Box::new(m20220101_000002_create_public_keys_table::Migration),
            Box::new(m20220101_000003_create_authentication_tokens_table::Migration),
            Box::new(m20220101_000004_create_nodes_table::Migration),
            Box::new(m20220101_000005_create_codes_table::Migration),
            Box::new(m20220101_000006_create_contracts_table::Migration),
            Box::new(m20220101_000007_create_source_codes_table::Migration),
            Box::new(m20220101_000008_create_files_table::Migration),
            Box::new(m20220101_000009_create_build_sessions_table::Migration),
            Box::new(m20220101_000010_create_build_session_tokens_table::Migration),
            Box::new(m20220101_000011_create_logs_table::Migration),
            Box::new(m20220101_000012_create_cli_tokens_table::Migration),
            Box::new(m20220101_000013_create_events_table::Migration),
            Box::new(m20220101_000014_remove_node_schema::Migration),
            Box::new(m20220101_000015_remove_rust_version::Migration),
            Box::new(m20220101_000016_add_project_directory::Migration),
            Box::new(m20220101_000017_create_diagnostics_table::Migration),
        ]
    }
}
