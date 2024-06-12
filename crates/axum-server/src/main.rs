use config::Config;
use elasticsearch::{
    auth::Credentials,
    http::transport::{SingleNodeConnectionPool, Transport, TransportBuilder},
    Elasticsearch,
};
use reqwest::Url;
use sqlx::{sqlite::SqlitePool, Pool, Sqlite};
mod config;
mod error;

use serde_json::{json, to_string_pretty, Value};

use axum::{
    body::Body,
    debug_handler,
    extract::Extension,
    response::Response,
    routing::{get, post},
    Router,
};
use serde::{self, Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::new();

    let pool = SqlitePool::connect(&config.database_url).await?;

    let app = Router::new()
        .route("/releases", get(releases))
        .route("/filters", get(filters))
        .route("/actions", post(actions))
        .layer(CorsLayer::permissive())
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

async fn actions(
    Extension(pool): Extension<Pool<Sqlite>>,
    params: axum_extra::extract::Query<Action>,
) -> Result<(), error::Error> {
    let mut client = pool.acquire().await?;

    let action = params.0.action.to_uppercase();

    sqlx::query!(
        "INSERT INTO actions VALUES (?, ?)",
        action,
        params.0.identifier
    )
    .execute(&mut *client)
    .await?;

    Ok(())
}

#[debug_handler]
async fn filters(
    Extension(config): Extension<Config>,
) -> Result<axum::response::Response, error::Error> {
    let credentials = Credentials::Basic("elastic".into(), config.es_password.clone());
    let transport = Transport::cloud(&config.es_cloud_id.clone(), credentials)?;

    let client = Elasticsearch::new(transport);

    let search = client
        .search(elasticsearch::SearchParts::None)
        .body(json!({
            "aggs": {
                "Styles": {
                    "terms": { "field": "styles.keyword", "size": 1000, "min_doc_count": 50 },
                    "meta": {
                        "field": "styles.keyword"
                    }
                },
                // "genres": {
                //     "terms": { "field": "genres.keyword", "size": 1000 }
                // },
                "Format": {
                    "terms": { "field": "formats.name.keyword", "size": 20 },
                    "meta": {
                        "field": "formats.name.keyword"
                    }
                },
                "Format Descriptions": {
                    "terms": { "field": "formats.descriptions.keyword", "size": 20 },
                    "meta": {
                        "field": "formats.descriptions.keyword"
                    }
                }
            },
            "size": 0
        }))
        .allow_no_indices(true)
        .send()
        .await?;

    let builder = Response::builder().status(200);

    let body = search.json::<Value>().await?;

    builder
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .map_err(|e| e.into())
}

#[derive(Debug, Deserialize, Serialize)]
struct Filters {
    field: Option<Vec<String>>,
    value: Option<Vec<String>>,
    from: Option<i64>,
    exclude: Option<Vec<i64>>,
}

async fn releases(
    Extension(pool): Extension<Pool<Sqlite>>,
    Extension(config): Extension<Config>,
    params: axum_extra::extract::Query<Filters>,
) -> Result<axum::response::Response, error::Error> {
    let credentials = Credentials::Basic("elastic".into(), config.es_password.clone());
    let transport = Transport::cloud(&config.es_cloud_id.clone(), credentials)?;

    let client = Elasticsearch::new(transport);

    // let search = elasticsearch_dsl::Search::new().size(10);
    // let resp: serde_json::Value = client
    //     .post("https://localhost:9200/releases")
    //     .json(&serde_json::to_string(&search).unwrap())
    //     .send()
    //     .await?
    //     .json()
    //     .await?;

    // dbg!(filters);

    let mut db = pool.acquire().await?;

    let actions = sqlx::query_as::<_, Action>("SELECT * FROM actions")
        .fetch_all(&mut *db)
        .await?;

    let json = json!({
        "query": {
            "bool": {
                "must": std::iter::zip(params.0.field.unwrap_or_default(), params.0.value.unwrap_or_default())
                    .map(|f| {
                        if f.0.contains("nested:")
                        {json!({"nested": {
                            "path": f.0[7..f.0.chars().position(|c| c == '.').unwrap()],
                            "query": {
                                "match_phrase_prefix": {
                                    f.0[7..]: {
                                        "query": f.1,
                                        // "fuzziness": "AUTO"
                                    }
                                }
                            }
                        }})}
                        else
                        {json!({ "match_phrase": { f.0: f.1 }})}
                    }).collect::<Vec<Value>>(),
                "must_not": [
                    {
                        "ids": {
                            "values": actions.iter().map(|a| &a.identifier).collect::<Vec<_>>()
                        }
                    }
                ]
            }
        },
        "sort": [
            {
                "id": {
                    "order": "desc"
                }
            }
        ]
    });

    println!("{}", to_string_pretty(&json).unwrap());

    let search = client
        .search(elasticsearch::SearchParts::Index(&["releases"]))
        .size(10)
        .from(params.0.from.unwrap_or(0))
        .body(json)
        .allow_no_indices(true)
        .send()
        .await?;

    let builder = Response::builder().status(200);

    let body = search.json::<Value>().await?;

    builder
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .map_err(|e| e.into())
}
