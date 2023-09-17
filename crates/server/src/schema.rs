use std::fmt::Display;

use axum::response::IntoResponse;
use common::rpc::sp_core::{
    crypto::{AccountId32, Ss58Codec},
    sr25519::{Pair, Public, Signature},
    Pair as _,
};
use db::{build_session, event::EventBody};
use serde_json::{json, Value};

use crate::hex_hash::HexHash;

/// Generate example values for OAPI documentation.
macro_rules! generate_examples {
    ($name:ident, $type:ty, $expr:expr) => {
        ::paste::paste! {
            #[doc = concat!("Generate example [`", stringify!($type), "`] value for OAPI documentation.")]
            pub(crate) fn [<example_ $name>]() -> $type {
                $expr
            }
        }
    };

    ($name:ident, $type:ty, $expr:expr; $($name_repeat:ident, $type_repeat:ty, $expr_repeat:expr);+) => {
        generate_examples!($name, $type, $expr);
        generate_examples!($($name_repeat, $type_repeat, $expr_repeat);+);
    }
}

/// Convert an error into a JSON value suitable for OAPI documentation.
pub(crate) fn example_error<E: Display + IntoResponse>(err: E) -> Value {
    let error = err.to_string();

    json! {{
        "code": err.into_response().status().as_u16(),
        "error": error,
    }}
}

generate_examples!(
    database_identifier, i64, 1;
    hex_hash, HexHash, HexHash([200; 32]);
    cargo_contract_version, String, String::from("4.0.0-alpha");
    build_session_status, build_session::Status, build_session::Status::Completed;
    log_position, Option<i64>, Some(40);
    log_entry, String, String::from("Compiling futures-util v0.3.28");
    timestamp, i64, 1672531200;
    account, AccountId32, AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap();
    public_key, Public, Public(example_account().into());
    signature, Signature, Pair::from_seed(&[0; 32]).sign(b"test message");
    token, String, String::from("UYEIngStyH6Bxu1hLFIIwBxLgyMBhMQv4SVR1KzzbvzIDCSMcwwF8ApXagqyuWbh");
    event_body, EventBody, EventBody::CodeHashUpdate {
        new_code_hash: hex::encode([200; 32]),
    };
    file, String, String::from("lib.rs");
    files, Vec<String>, vec![
        String::from("lib.rs"),
        String::from("Cargo.toml"),
        String::from("Cargo.lock"),
    ];
    folder, Option<String>, Some(String::from("contracts/test_contract"));
    node, String, String::from("alephzero")
);
