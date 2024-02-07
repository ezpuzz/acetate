#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_web::Config;
use log::LevelFilter;

use serde::Deserialize;
use serde_json::Value;

fn main() {
    // Init debug
    console_error_panic_hook::set_once();
    dioxus_logger::init(LevelFilter::Info).expect("failed to init logger");

    // now rehydrate
    dioxus_web::launch::launch(|| rsx!(Router::<Route> {}), vec![], Config::new());
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
struct ElasticResult {
    _id: String,
    _source: RecordDetails,
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
struct RecordDetails {
    title: String,
    artists: Vec<Value>,
    styles: Vec<String>,
}

async fn get_releases() -> Result<Vec<ElasticResult>, reqwest::Error> {
    let resp = reqwest::Client::new()
        .get("http://localhost:3000/releases")
        .send()
        .await?
        .json::<Value>()
        .await?;

    let hits = &resp["hits"]["hits"];

    let records: Vec<ElasticResult> = serde_json::from_value(hits.clone()).unwrap();

    log::info!("help");
    log::info!("{records:#?}");
    Ok(records)
}

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home { r: ElasticResult },
}

#[component]
fn Home(r: ElasticResult) -> Element {
    let releases = use_resource(move || get_releases());

    match releases.read().as_ref() {
        Some(Ok(list)) => {
            rsx! {
                div {
                    for r in list {
                        RecordDisplay { r: r.clone() }
                    }
                }
            }
        }
        Some(Err(e)) => rsx! { "{e.to_string()}" },
        None => rsx! {"Loading"},
    }
    // rsx! {
    //     Link { to: Route::Home {}, "Go to counter" }
    //     "Blog post {id}"
    // }
}

#[component]
fn RecordDisplay(r: ElasticResult) -> Element {
    let artists = r._source.artists;

    rsx! {
        div{
            style: "padding: 10px;",
            div {
                for a in artists {
                    "{a[\"name\"].as_str().unwrap()}"
                    " {a[\"join\"].as_str().unwrap_or(\"\")} "
                }

                " - {r._source.title}"
            }
            div {
                "{r._source.styles.join(\", \")}"
            }
        }
    }
}
