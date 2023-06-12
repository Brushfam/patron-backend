//! CLI token used for authentication token exchanges.
//!
//! For CLI authentication, the following schema is used:
//!
//! 1. CLI generates a random string of [`TOKEN_LENGTH`] length.
//! 2. CLI sends a web request for user authentication with the generated token.
//! 3. As soon as authentication is successful,
//! CLI can call a dedicated method to exchange
//! the generated token for an authentication token.

use sea_orm::entity::prelude::*;

pub const TOKEN_LENGTH: usize = 64;

/// CLI exchange token info model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "cli_tokens")]
pub struct Model {
    /// Unique CLI token string.
    #[sea_orm(primary_key)]
    pub token: String,

    /// Related authentication token identifier.
    pub authentication_token_id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::token::Entity",
        from = "Column::AuthenticationTokenId",
        to = "super::token::Column::Id"
    )]
    AuthenticationToken,
}

impl Related<super::token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AuthenticationToken.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
