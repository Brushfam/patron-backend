use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use sea_orm::{entity::prelude::*, ActiveValue};
use time::{Duration, OffsetDateTime, PrimitiveDateTime};

pub const TOKEN_LENGTH: usize = 64;
pub const TOKEN_LIFESPAN: Duration = Duration::weeks(12);

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "authentication_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub token: String,
    pub created_at: TimeDateTime,
}

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

pub fn generate_token(user_id: i64) -> (ActiveModel, String) {
    let token = Alphanumeric.sample_string(&mut thread_rng(), TOKEN_LENGTH);

    let now = OffsetDateTime::now_utc();

    let created_at = PrimitiveDateTime::new(now.date(), now.time());

    (
        ActiveModel {
            user_id: ActiveValue::Set(user_id),
            token: ActiveValue::Set(token.clone()),
            created_at: ActiveValue::Set(created_at),
            ..Default::default()
        },
        token,
    )
}
