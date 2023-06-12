//! A single source code file stored in the uploaded archive.
//!
//! The files themselves are discovered inside of an isolated container
//! and are sent to an API server via separate requests.

use sea_orm::entity::prelude::*;

/// Source code file model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "files")]
pub struct Model {
    /// Unique file identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Related source code identifier.
    pub source_code_id: i64,

    /// File path within the uploaded archive.
    pub name: String,

    /// File contents.
    pub text: String,
}

/// File model relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::source_code::Entity",
        from = "Column::SourceCodeId",
        to = "super::source_code::Column::Id"
    )]
    SourceCode,
}

impl Related<super::source_code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceCode.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
