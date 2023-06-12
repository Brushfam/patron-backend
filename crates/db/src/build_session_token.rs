//! Build session token.
//!
//! These tokens are used to exchange information about
//! source code files with an API server in a safe manner.
//!
//! As soon as all files are passed to an API server
//! the build session token should be destroyed by calling
//! a "seal" method on an API server.

use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use sea_orm::entity::prelude::*;

/// Build session token length.
pub const TOKEN_LENGTH: usize = 64;

/// Build session token model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "build_session_tokens")]
pub struct Model {
    /// Unique build session token value.
    #[sea_orm(primary_key)]
    pub token: String,

    /// Related source code identifier.
    ///
    /// This identifier is present here for easier fetching
    /// without triggering any transaction locks on the build session
    /// itself.
    pub source_code_id: i64,

    /// Related build session identifier
    pub build_session_id: i64,
}

/// Build session token relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::source_code::Entity",
        from = "Column::SourceCodeId",
        to = "super::source_code::Column::Id"
    )]
    SourceCode,

    #[sea_orm(
        belongs_to = "super::build_session::Entity",
        from = "Column::BuildSessionId",
        to = "super::build_session::Column::Id"
    )]
    BuildSession,
}

impl Related<super::source_code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceCode.def()
    }
}

impl Related<super::build_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BuildSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

/// Generate a random build session token string value.
///
/// The length is guaranteed to be equal to [`TOKEN_LENGTH`].
pub fn generate_token() -> String {
    Alphanumeric.sample_string(&mut thread_rng(), TOKEN_LENGTH)
}
