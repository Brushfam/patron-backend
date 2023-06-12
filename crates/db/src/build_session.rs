//! A single contract build session.
//!
//! This model is a core model that handles the information on
//! the contract build process itself.
//!
//! It contains all the necessary information on the related contract source code,
//! Rust and `cargo-contract` tooling versions, and, as soon as the build is successful,
//! WASM code hash and JSON metadata.

use sea_orm::{entity::prelude::*, FromQueryResult};
use serde::Serialize;

/// Build session model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "build_sessions")]
pub struct Model {
    /// Unique build session identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Identifier of a user that initiated a build session.
    ///
    /// [`None`] if a user was previously deleted.
    pub user_id: Option<i64>,

    /// Related contract source code identifier.
    pub source_code_id: i64,

    /// Current build session [`Status`].
    pub status: Status,

    /// `cargo-contract` tooling version.
    pub cargo_contract_version: String,

    /// Rust tooling version.
    pub rustc_version: String,

    /// WASM blob code hash, if the contract build was successful.
    pub code_hash: Option<Vec<u8>>,

    /// JSON metadata value, if the contract build was successful.
    pub metadata: Option<Vec<u8>>,

    /// Build session creation time.
    pub created_at: TimeDateTime,
}

/// Build session status.
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize)]
#[sea_orm(rs_type = "i16", db_type = "Integer")]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Build session has not started yet or is in progress
    /// if the related row is locked.
    #[sea_orm(num_value = 0)]
    New,

    /// An attempt to build the contract failed.
    ///
    /// More information about fail reasons is available in logs.
    #[sea_orm(num_value = 1)]
    Failed,

    /// Build session finished successfully.
    #[sea_orm(num_value = 2)]
    Completed,
}

/// Build session relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::code::Entity",
        from = "Column::CodeHash",
        to = "super::code::Column::Hash"
    )]
    Code,

    #[sea_orm(
        belongs_to = "super::source_code::Entity",
        from = "Column::SourceCodeId",
        to = "super::source_code::Column::Id"
    )]
    SourceCode,

    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Code.def()
    }
}

impl Related<super::source_code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceCode.def()
    }
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

/// Information about the build session necessary to
/// start its processing.
#[derive(FromQueryResult)]
pub struct ProcessedBuildSession {
    pub id: i64,
    pub source_code_id: i64,
    pub rustc_version: String,
    pub cargo_contract_version: String,
}

/// Build session info used to provide details to users.
#[derive(Serialize, FromQueryResult)]
pub struct BuildSessionInfo {
    pub source_code_id: i64,
    pub cargo_contract_version: String,
    pub rustc_version: String,
}
