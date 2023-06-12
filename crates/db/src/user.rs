//! Registered user.
//!
//! User registration should be managed transparently with the authentication flow,
//! since not much information about user is collected at all, thus allowing us
//! to seamlessly register new users and automatically attach public keys to them
//! for later authentications.

use sea_orm::entity::prelude::*;

/// User model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub paid: bool,
    pub created_at: TimeDateTime,
}

/// User model relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::public_key::Entity")]
    PublicKeys,

    #[sea_orm(has_many = "super::token::Entity")]
    Tokens,

    #[sea_orm(has_many = "super::source_code::Entity")]
    SourceCodes,

    #[sea_orm(has_many = "super::build_session::Entity")]
    BuildSessions,
}

impl Related<super::public_key::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PublicKeys.def()
    }
}

impl Related<super::token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tokens.def()
    }
}

impl Related<super::source_code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceCodes.def()
    }
}

impl Related<super::build_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tokens.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
