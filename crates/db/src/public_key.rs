//! Verified public key associated with a single user.
//!
//! Public keys information is required to authenticate users in a simple
//! and wallet vendor-independent manner.
//!
//! Public key verification is done by signing some generated message
//! and verifying that the signature corresponds to the requested public key value.

use sea_orm::entity::prelude::*;

/// Public key model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "public_keys")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub address: Vec<u8>,
    pub created_at: TimeDateTime,
}

/// Public key model relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
