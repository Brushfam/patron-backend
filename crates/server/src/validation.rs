use axum::{
    async_trait,
    extract::{rejection::JsonRejection, FromRequest},
    http::{Request, StatusCode},
    Json,
};
use axum_derive_error::ErrorResponse;
use derive_more::{Display, Error};
use validator::{Validate, ValidationErrors};

/// Errors related to JSON validation.
#[derive(ErrorResponse, Display, Error)]
pub enum ValidatedJsonRejection {
    /// Unable to parse a JSON value.
    #[status(StatusCode::UNPROCESSABLE_ENTITY)]
    JsonParsingError(JsonRejection),

    /// Unable to validate a JSON value.
    #[status(StatusCode::UNPROCESSABLE_ENTITY)]
    ValidationError(ValidationErrors),
}

/// Wrapper for [`axum`] JSON value validation.
///
/// Equivalent to the [`axum`]'s [`Json`] struct
/// with [`validator`] crate support.
///
/// [`JSON`]: axum::extract::Json
pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<T, S, B> FromRequest<S, B> for ValidatedJson<T>
where
    T: Validate,
    B: Send + 'static,
    S: Sync,
    Json<T>: FromRequest<S, B, Rejection = JsonRejection>,
{
    type Rejection = ValidatedJsonRejection;

    async fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::from_request(req, state)
            .await
            .map_err(ValidatedJsonRejection::JsonParsingError)?;

        match value.validate() {
            Ok(_) => Ok(ValidatedJson(value)),
            Err(err) => Err(ValidatedJsonRejection::ValidationError(err)),
        }
    }
}
