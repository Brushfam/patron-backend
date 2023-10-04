use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "diagnostics")]
pub struct Model {
    /// Unique diagnostic identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Related build session identifier.
    pub build_session_id: i64,

    /// Related file identifier.
    pub file_id: i64,

    /// Diagnostic level.
    pub level: Level,

    /// Diagnostic start file position.
    pub start: i64,

    /// Diagnostic end file position.
    pub end: i64,

    /// Diagnostic message.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, JsonSchema)]
#[sea_orm(rs_type = "i16", db_type = "Integer")]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// An error was found, which prevents any build attempts.
    #[sea_orm(num_value = 0)]
    Error,

    /// A warning was found, which may prevent any build attempts.
    #[sea_orm(num_value = 1)]
    Warning,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::FileId",
        to = "super::file::Column::Id"
    )]
    File,

    #[sea_orm(
        belongs_to = "super::build_session::Entity",
        from = "Column::BuildSessionId",
        to = "super::build_session::Column::Id"
    )]
    BuildSession,
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl Related<super::build_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BuildSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
