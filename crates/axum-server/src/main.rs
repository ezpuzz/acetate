use elasticsearch::{
    auth::Credentials,
    http::transport::{SingleNodeConnectionPool, Transport, TransportBuilder},
    Elasticsearch,
};
use reqwest::Url;
use sqlx::{sqlite::SqlitePool, Pool, Sqlite};
mod config;
mod error;
use crate::error::ResultExt;

use serde_json::{json, Value};

use axum::{
    body::Body,
    debug_handler,
    extract::{Extension, State},
    response::{Json, Response},
    routing::get,
    Router,
};
use serde;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::new();

    let pool = SqlitePool::connect(&config.database_url).await?;

    let client = reqwest::Client::new();

    let app = Router::new()
        .route("/releases", get(releases))
        .route("/actions", get(actions))
        .layer(Extension(config))
        .layer(Extension(pool.clone()))
        .with_state(client);

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

#[debug_handler]
async fn releases(
    State(client): State<reqwest::Client>,
) -> Result<axum::response::Response, error::Error> {
    let credentials = Credentials::Basic("elastic".into(), "FsW*tVgYYSvVVagE03*c".into());
    let url = Url::parse("https://localhost:9200").unwrap();
    let conn_pool = SingleNodeConnectionPool::new(url);
    let transport = TransportBuilder::new(conn_pool)
        .disable_proxy()
        .auth(credentials)
        .build()
        .unwrap();

    let client = Elasticsearch::new(transport);

    // let search = elasticsearch_dsl::Search::new().size(10);
    // let resp: serde_json::Value = client
    //     .post("https://localhost:9200/releases")
    //     .json(&serde_json::to_string(&search).unwrap())
    //     .send()
    //     .await?
    //     .json()
    //     .await?;

    let search = client
        .search(elasticsearch::SearchParts::None)
        .body(json!({
            "query": {
                "match_all": {}
            }
        }))
        .allow_no_indices(true)
        .send()
        .await?;

    let mut builder = Response::builder().status(200);

    let headers = search.headers();

    for h in headers.into_iter() {
        builder = builder.header(h.0.as_str(), h.1.to_str().unwrap());
    }

    let body = search.json::<Value>().await?;

    builder
        .body(Body::from(body.to_string()))
        .map_err(|e| e.into())
}
