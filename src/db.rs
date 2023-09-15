use thiserror::Error;

use diesel::prelude::*;
use diesel_async::{
    pooled_connection::{AsyncDieselConnectionManager, PoolError},
    AsyncPgConnection, RunQueryDsl,
};

use crate::schema::messages;

type Pool = bb8::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Env error: {0}")]
    Env(#[from] std::env::VarError),
    #[error("Pool error: {0}")]
    Pool(#[from] bb8::RunError<PoolError>),
    #[error("Database error: {0}")]
    Database(#[from] diesel::result::Error),
}

type DbResult<T> = Result<T, DbError>;

#[derive(Queryable, Selectable)]
#[diesel(table_name = messages)]
pub struct Message {
    pub id: i32,
    pub message: Option<String>,
    pub updated: Option<i32>,
}

#[derive(Clone)]
pub struct Db {
    conn_pool: Pool,
}

impl Db {
    pub async fn new(db_url: &str) -> DbResult<Self> {
        let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(db_url);
        let conn_pool = bb8::Pool::builder().build(config).await.unwrap();

        Ok(Self { conn_pool })
    }

    pub async fn get_messages(&self) -> DbResult<Vec<Message>> {
        let mut conn = self.conn_pool.get().await?;
        Ok(messages::table.load::<Message>(&mut conn).await?)
    }

    pub async fn insert_message(&self, message: &str) -> DbResult<Message> {
        let mut conn = self.conn_pool.get().await?;
        let user = diesel::insert_into(messages::table)
            .values(messages::message.eq(message))
            .get_result(&mut conn)
            .await?;

        Ok(user)
    }
}
