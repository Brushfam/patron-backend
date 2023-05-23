use db::build_session::Status;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BuildSessions::Table)
                    .col(
                        ColumnDef::new(BuildSessions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BuildSessions::UserId).big_integer())
                    .col(
                        ColumnDef::new(BuildSessions::SourceCodeId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BuildSessions::CargoContractVersion)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BuildSessions::RustcVersion)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(BuildSessions::CodeHash).binary())
                    .col(
                        ColumnDef::new(BuildSessions::Status)
                            .small_integer()
                            .not_null()
                            .default(Status::New),
                    )
                    .col(ColumnDef::new(BuildSessions::Metadata).binary())
                    .col(
                        ColumnDef::new(BuildSessions::CreatedAt)
                            .timestamp()
                            .not_null()
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(BuildSessions::Table, BuildSessions::UserId)
                            .to(crate::Users::Table, crate::Users::Id)
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(BuildSessions::Table, BuildSessions::SourceCodeId)
                            .to(crate::SourceCodes::Table, crate::SourceCodes::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BuildSessions::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub(crate) enum BuildSessions {
    Table,
    Id,
    UserId,
    SourceCodeId,
    CargoContractVersion,
    RustcVersion,
    CodeHash,
    Status,
    Metadata,
    CreatedAt,
}
