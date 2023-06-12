//! Recorded log record.
//!
//! The text inside of a single recorded log model instance is not guaranteed
//! to be equivalent to a single line of a container log output.
//!
//! To correctly display log output either manually split lines or output
//! [`Model`]'s `text` field as-is.

use sea_orm::entity::prelude::*;

/// Log record model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "logs")]
pub struct Model {
    /// Unique log identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Related build session identifier.
    pub build_session_id: i64,

    /// Log record text value.
    pub text: String,
}

/// Log record model relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::build_session::Entity",
        from = "Column::BuildSessionId",
        to = "super::build_session::Column::Id"
    )]
    BuildSession,
}

impl Related<super::build_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BuildSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
