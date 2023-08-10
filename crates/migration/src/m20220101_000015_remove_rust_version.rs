use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BuildSessions::Table)
                    .drop_column(BuildSessions::RustcVersion)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BuildSessions::Table)
                    .add_column(
                        ColumnDef::new(BuildSessions::RustcVersion)
                            .string()
                            .not_null()
                            .default("0.0.0"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(BuildSessions::Table)
                    .modify_column(
                        ColumnDef::new(BuildSessions::RustcVersion)
                            .string()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub(crate) enum BuildSessions {
    Table,
    RustcVersion,
}
