use config::Config;
use elasticsearch::{auth::Credentials, http::transport::Transport, Elasticsearch};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
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
use dotenv::dotenv;
use serde::{self, Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    let config = config::Config::new();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    let credentials = Credentials::Basic("elastic".into(), config.es_password.clone());
    let transport = Transport::cloud(&config.es_cloud_id.clone(), credentials)?;
    let client = Elasticsearch::new(transport);

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let app = Router::new()
        .route("/releases", get(releases))
        .route("/filters", get(filters))
        .route("/actions", post(actions))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .layer(Extension(config))
        .layer(Extension(client))
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
    Extension(pool): Extension<Pool<Postgres>>,
    params: axum_extra::extract::Query<Action>,
) -> Result<(), error::Error> {
    let mut client = pool.acquire().await?;

    let action = params.0.action.to_uppercase();

    let user = 0;

    sqlx::query!(
        "INSERT INTO actions VALUES ($1, $2, $3)",
        user,
        action,
        params.0.identifier
    )
    .execute(&mut *client)
    .await?;

    Ok(())
}

#[debug_handler]
async fn filters(
    Extension(client): Extension<Elasticsearch>,
) -> Result<axum::response::Response, error::Error> {
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
struct QueryParameters {
    field: Option<Vec<String>>,
    value: Option<Vec<String>>,
    from: Option<i64>,
    size: Option<i64>,
    videos_only: Option<bool>,
}

async fn releases(
    Extension(pool): Extension<Pool<Postgres>>,
    Extension(client): Extension<Elasticsearch>,
    params: axum_extra::extract::Query<QueryParameters>,
) -> Result<axum::response::Response, error::Error> {
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

    let mut must_filters = vec![];
    let mut filters = vec![];

    std::iter::zip(
        params.0.field.unwrap_or_default(),
        params.0.value.unwrap_or_default(),
    )
    .for_each(|f| {
        if f.0.contains("nested:") {
            filters.push(json!({"nested": {
                "path": f.0[7..f.0.chars().position(|c| c == '.').unwrap()],
                "query": {
                    "bool": {
                        "should": [
                            {   "fuzzy": {
                                    f.0[7..]: {
                                        "value": format!("{0}",f.1),
                                        "prefix_length": 1,
                                        "boost": "0.5"
                                    }
                                }
                            },
                            {   "match": {
                                    f.0[7..]: {
                                        "query": format!("{0}",f.1),
                                        "operator": "and",
                                        "boost": "0.5"
                                    }
                                }
                            },
                            {   "wildcard": {
                                    f.0[7..]: {
                                        "value": format!("*{0}*",f.1),
                                        "case_insensitive": true,
                                        "boost": "10.0"
                                    }
                                }
                            },
                            {   "wildcard": {
                                    f.0[7..]: {
                                        "value": format!("{0}*",f.1),
                                        "case_insensitive": true,
                                        "boost": "15.0"
                                    }
                                }
                            }
                        ],
                        "boost": match &f.0[7..]{
                            "labels.catno.keyword" => "15.0",
                            _ => "1.0"
                        }
                    },
                },
                "score_mode": "max"
            }}))
        } else {
            must_filters.push(json!({ "match": { f.0: f.1 }}));
        }
    });

    if params.0.videos_only.unwrap_or(false) {
        must_filters.append(&mut vec![json!({ "exists": {
            "field": "videos"
        }})]);
    }

    let json = json!({
        "query": {
            "bool": {
                "must": filters,
                "filter": must_filters,
                "must_not": [
                    {
                        "ids": {
                            "values": actions.iter().map(|a| &a.identifier).collect::<Vec<_>>()
                        }
                    },
                    // Note: the following avoids duplicates but hides remixes, needs smarter filtering to avoid lots of dupes
                    // Workaround: allow hiding all of a particular release with some kind of confirmation?
                    // {
                    //     "term": {
                    //         "master_id.is_main_release": "false"
                    //       }
                    // }
                ],
                // "minimum_should_match": 1
            }
        },
        "sort": [
            {
                "_score": {
                    "order": "desc"
                }
            },
            {
                "released": {
                    "order": "desc"
                }
            }
        ],
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
    });

    println!("{}", to_string_pretty(&json).unwrap());

    let search = client
        .search(elasticsearch::SearchParts::Index(&["releases"]))
        .size(params.0.size.unwrap_or(10))
        .from(params.0.from.unwrap_or(0))
        .body(json)
        .send()
        .await?;

    let builder = Response::builder().status(200);

    let body = search.json::<Value>().await?;

    builder
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .map_err(|e| e.into())
}
