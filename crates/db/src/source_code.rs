//! Source code archive.
//!
//! Source code archives are uploaded using CLI and are unpacked
//! during the smart contract build process inside of an isolated container.
//!
//! There are no guarantees related to the archive itself, thus the archive unpacking
//! should only be performed in isolated environments.

use sea_orm::entity::prelude::*;

/// Source code archive model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "source_codes")]
pub struct Model {
    /// Unique source code archive identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Related user identifier that uploaded this source code.
    ///
    /// [`None`] if a user was previously deleted.
    pub user_id: Option<i64>,

    /// Blake2b 256-bit archive hash.
    pub archive_hash: Vec<u8>,

    /// Source code archive upload timestamp.
    pub created_at: TimeDateTime,
}

/// Source code archive model relations.
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
