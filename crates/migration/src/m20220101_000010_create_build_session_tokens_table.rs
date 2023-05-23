use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BuildSessionTokens::Table)
                    .col(
                        ColumnDef::new(BuildSessionTokens::Token)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BuildSessionTokens::SourceCodeId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BuildSessionTokens::BuildSessionId)
                            .big_integer()
                            .not_null()
                            .unique_key(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(BuildSessionTokens::Table, BuildSessionTokens::SourceCodeId)
                            .to(crate::SourceCodes::Table, crate::SourceCodes::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                BuildSessionTokens::Table,
                                BuildSessionTokens::BuildSessionId,
                            )
                            .to(crate::BuildSessions::Table, crate::BuildSessions::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BuildSessionTokens::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum BuildSessionTokens {
    Table,
    Token,
    SourceCodeId,
    BuildSessionId,
}
