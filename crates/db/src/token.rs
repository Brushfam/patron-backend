//! User authentication token.
//!
//! Authentication token is passed to an API server to identify
//! a user that executes the request.
//!
//! Authentication tokens have their lifespan limited to [`TOKEN_LIFESPAN`] [`Duration`]
//! value, and are to have their length equal to the [`TOKEN_LENGTH`] value.

use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use sea_orm::{entity::prelude::*, ActiveValue};
use time::{Duration, OffsetDateTime, PrimitiveDateTime};

pub const TOKEN_LENGTH: usize = 64;
pub const TOKEN_LIFESPAN: Duration = Duration::weeks(12);

/// Authentication token model.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "authentication_tokens")]
pub struct Model {
    /// Unique authentication token identifier.
    #[sea_orm(primary_key)]
    pub id: i64,

    /// Related user identifier.
    pub user_id: i64,

    /// Authentication token string value.
    pub token: String,

    /// Authentication token creation timestamp.
    pub created_at: TimeDateTime,
}

/// Authentication token model relations.
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

/// Generate new authentication token for the provided user identifier.
///
/// This function returns both an [`ActiveModel`] of an authentication token
/// and its string value.
///
/// ## Example
///
/// ```
/// use db::token::{TOKEN_LENGTH, generate_token};
///
/// let (_, token_string) = generate_token(1);
/// assert_eq!(token_string.len(), TOKEN_LENGTH);
/// ```
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
