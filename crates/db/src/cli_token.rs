use sea_orm::entity::prelude::*;

pub const TOKEN_LENGTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "cli_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub token: String,
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
