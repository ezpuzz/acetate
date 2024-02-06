use sqlx::{sqlite::SqlitePool, Pool, Sqlite};
mod config;
mod error;
use crate::error::ResultExt;

use axum::{debug_handler, extract::Extension, response::Json, routing::get, Router};
use serde;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::new();

    let pool = SqlitePool::connect(&config.database_url).await?;

    let app = Router::new()
        .route("/actions", get(actions))
        .layer(Extension(config))
        .layer(Extension(pool.clone()));

    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Actions {
    actions: Vec<Action>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
struct Action {
    action: String,
    identifier: String,
}

#[debug_handler]
async fn actions(Extension(pool): Extension<Pool<Sqlite>>) -> Result<Json<Actions>, error::Error> {
    let mut client = pool.acquire().await?;
    sqlx::query!("INSERT INTO actions VALUES ('HIDE', '1')")
        .execute(&mut *client)
        .await?;

    let actions = sqlx::query_as::<_, Action>("SELECT * FROM actions")
        .fetch_all(&mut *client)
        .await
        .on_constraint("asdf", |_| error::Error::Conflict)?;

    dbg!(actions.clone());
    Ok(Json(Actions { actions }))
}
