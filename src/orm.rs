#[allow(warnings, clippy::all)]
pub(crate) mod short_url {
    use sea_orm::entity::prelude::*;
    use time::OffsetDateTime;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "urls")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: String,
        pub long_url: String,
        pub expiration_time_seconds: TimeUnixTimestamp,
    }

    impl ActiveModelBehavior for ActiveModel {}
}
