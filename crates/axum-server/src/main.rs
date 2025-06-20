use base64::Engine;
use elasticsearch::{auth::Credentials, http::transport::Transport, Elasticsearch};
use roaring::RoaringBitmap;
mod config;
mod error;

use serde_json::{json, to_string_pretty, Value};

use axum::{
    body::Body, debug_handler, extract::Extension, response::Response, routing::get, Router,
};
use dotenv::dotenv;
use serde::{self, Deserialize, Deserializer, Serialize};
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    let config = config::Config::new();

    let credentials = Credentials::Basic("elastic".into(), config.es_password.clone());
    let transport = Transport::cloud(&config.es_cloud_id.clone(), credentials)?;
    let client = Elasticsearch::new(transport);

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let app = Router::new()
        .route("/releases", get(releases))
        .route("/release", get(release))
        .route("/filters", get(filters))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .layer(Extension(config))
        .layer(Extension(client));
    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    // println!("listening on {}", addr);
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
async fn filters(
    Extension(client): Extension<Elasticsearch>,
) -> Result<axum::response::Response, error::Error> {
    let search = client
        .search(elasticsearch::SearchParts::None)
        .body(json!({
            "aggs": {
                "Styles": {
                    "terms": { "field": "styles", "size": 1000, "min_doc_count": 50 },
                    "meta": {
                        "field": "styles"
                    }
                },
                // "genres": {
                //     "terms": { "field": "genres", "size": 1000 }
                // },
                "Format": {
                    "terms": { "field": "formats.name", "size": 20 },
                    "meta": {
                        "field": "formats.name"
                    }
                },
                "Format Descriptions": {
                    "terms": { "field": "formats.descriptions", "size": 20 },
                    "meta": {
                        "field": "formats.descriptions"
                    }
                },
                "Country": {
                    "terms": {"field": "country", "size": 50},
                    "meta": {
                        "field": "country"
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
    masters_only: Option<bool>,
    #[serde(default, deserialize_with = "from_base64")]
    hide: Option<RoaringBitmap>,
    search_after: Option<String>,
}

fn from_base64<'a, D>(deserializer: D) -> Result<Option<RoaringBitmap>, D::Error>
where
    D: Deserializer<'a>,
{
    use serde::de::Error;
    String::deserialize(deserializer).and_then(|string| {
        base64::engine::general_purpose::URL_SAFE
            .decode(&string)
            .map_err(|e| Error::custom(e.to_string()))
            .and_then(|bytes| Ok(Some(RoaringBitmap::deserialize_from(&*bytes).unwrap())))
    })
}

async fn releases(
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

    let mut filter = vec![];
    let mut must_not = vec![];

    let mut must = vec![];
    let mut should = vec![];

    std::iter::zip(
        params.0.field.unwrap_or_default(),
        params.0.value.unwrap_or_default(),
    )
    .for_each(|f| {
        if f.0.contains("nested:artists") { // searches on artist or extraartist
            should.push(json!({
            "nested": {
                "path": "artists",
                "query": {
                    "bool": {
                        "should": [
                            {
                                "match": {"artists.name": {"query": f.1, "boost": 0, "operator": "AND"}}
                            },
                            {
                                "match": {"artists.anv": {"query": f.1, "boost": 0, "operator": "AND"}}
                            },
                            ]
                        }
                    }
                }
            }));
            should.push(json!({
                "nested": {
                    "path": "extraartists",
                    "query": {
                        "bool": {
                            "should": [
                                {
                                    "match": {"extraartists.name": {"query": f.1, "boost": 0, "operator": "AND"}}
                                },
                                {
                                    "match": {"extraartists.anv": {"query": f.1, "boost": 0, "operator": "AND"}}
                                },

                            ]
                        }
                    }
                }
            }));
        } else if f.0.contains("nested:") { // searches on other nested fields
            must.push(json!({"nested": {
                "path": f.0[7..f.0.chars().position(|c| c == '.').unwrap()],
                "query": {
                    "bool": {
                        "should": [
                            {   "fuzzy": {
                                    f.0[7..]: {
                                        "value": format!("{0}",f.1),
                                        "fuzziness": 2,
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
                            // {   "prefix": {
                            //         f.0[7..]: {
                            //             "value": f.1,
                            //             "case_insensitive": true,
                            //             "boost": "15.0"
                            //         }
                            //     }
                            // }
                        ],
                        "boost": match &f.0[7..]{
                            "labels.catno" => "15.0",
                            _ => "1.0"
                        }
                    },
                },
                "score_mode": "max"
            }}))
        } else if f.0 == "title" { // searches on title
            must.push(json!({
                "match_bool_prefix": { f.0: {
                    "query": f.1,
                    "operator": "AND",
                    "boost": "10.0"
                } }
            }));
        } else { // searches on any other field
            filter.push(json!({ "term": { f.0: f.1 }}));
        }
    });

    if params.0.videos_only.unwrap_or(false) {
        filter.append(&mut vec![json!({ "exists": {
            "field": "videos"
        }})]);
    }

    if params.0.masters_only.unwrap_or(false) {
        // Note: the following avoids duplicates but hides remixes, needs smarter filtering to avoid lots of dupes
        // Workaround: allow hiding all of a particular release with some kind of confirmation?
        must_not.append(&mut vec![json!({
            "term": {
                "master_id.is_main_release": false
            }
        })]);
    }

    if params.0.hide.is_some() {
        // following needs the plugin to work on ES
        // must_filters.push(json!(
        //     {
        //         "script": {
        //             "script": {
        //                 "source": "fast_filter",
        //                 "lang": "fast_filter",
        //                 "params": {
        //                     "field": "id",
        //                     "operation": "exclude",
        //                     "terms": params.0.hide.unwrap()
        //                 }
        //             }
        //         }
        //     }
        // ))
        must_not.push(json!({
                    "terms": {
                        "_id": params.0.hide.unwrap().iter().collect::<Vec<u32>>()
                    }
        }));
    }

    let mut json = json!({
        "query": {
            "bool": {
                "must": must,
                "filter": filter,
                "must_not": must_not,
                "should": should,
                "minimum_should_match": if !should.is_empty() { 1 } else { 0 }
            }
        },
        "sort": [
            // _score sorting is prepended when needed later
            {
                "released": {
                    "order": "desc"
                }
            },
            {
                "_doc": {}
            }
        ],
        // The below is cool but really slow.
        // "aggs": {
        //     "Styles": {
        //         "terms": { "field": "styles", "size": 1000, "min_doc_count": 50 },
        //         "meta": {
        //             "field": "styles"
        //         }
        //     },
        //     // "genres": {
        //     //     "terms": { "field": "genres", "size": 1000 }
        //     // },
        //     "Format": {
        //         "terms": { "field": "formats.name", "size": 20 },
        //         "meta": {
        //             "field": "formats.name"
        //         }
        //     },
        //     "Format Descriptions": {
        //         "terms": { "field": "formats.descriptions", "size": 20 },
        //         "meta": {
        //             "field": "formats.descriptions"
        //         }
        //     }
        // },
    });

    if !should.is_empty() || !must.is_empty() {
        json["sort"].as_array_mut().unwrap().insert(
            0,
            json!({
                "_score": {
                    "order": "desc"
                }
            }),
        );
    }

    // println!("{:?}", params.0.search_after);
    if params.0.search_after.is_some() {
        json["search_after"] = serde_json::from_str(&params.0.search_after.unwrap()).unwrap();
    }

    tracing::debug!("{}", to_string_pretty(&json).unwrap());

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

#[derive(Debug, Deserialize, Serialize)]
struct ReleaseQueryParameters {
    id: String,
}

async fn release(
    Extension(client): Extension<Elasticsearch>,
    params: axum_extra::extract::Query<ReleaseQueryParameters>,
) -> Result<axum::response::Response, error::Error> {
    let search = client
        .get(elasticsearch::GetParts::IndexId("releases", &params.0.id))
        .send()
        .await?;

    let builder = Response::builder().status(200);

    let body = search.json::<Value>().await?;

    builder
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .map_err(|e| e.into())
}
