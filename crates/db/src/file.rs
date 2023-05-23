use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub source_code_id: i64,
    pub name: String,
    pub text: String,
}

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
