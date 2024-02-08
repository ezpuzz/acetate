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
    tracklist: Vec<Value>,
    videos: Option<Vec<String>>,
}

struct FilterValue {
    count: i64,
    label: String,
}

struct Filter {
    name: String,
    values: Vec<FilterValue>,
}

async fn get_filters() -> Result<Vec<String>, reqwest::Error> {
    let resp = reqwest::Client::new()
        .get("http://localhost:3000/filters")
        .send()
        .await?
        .json::<Value>()
        .await?;

    let filters = resp["aggregations"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| log::info!("{key}"));

    Ok(resp["aggregations"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|h| h.to_string())
        .collect::<Vec<String>>())
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
    let styles = use_resource(|| get_filters());

    rsx! {
        div {
            match styles.read().as_ref() {
                Some(Ok(styles)) => {
                    rsx!{
                        for s in styles {
                            "{s}"
                        }
                    }
                }
                Some(Err(e)) => rsx! { "{e.to_string()}" },
                None => rsx! {"Loading"},
            }
        }


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
    }
    // rsx! {
    //     Link { to: Route::Home {}, "Go to counter" }
    //     "Blog post {id}"
    // }
}

#[component]
fn RecordDisplay(r: ElasticResult) -> Element {
    let embed = r
        ._source
        .videos
        .unwrap_or(vec![])
        .iter()
        .map(|url| &url[32..])
        .collect::<Vec<&str>>()
        .join(",");

    rsx! {
        div {
            class: "flex pb-4",
            div {
                class: "p-4 w-1/2",
                div {
                    for a in r._source.artists {
                        "{a[\"name\"].as_str().unwrap()}"
                        " {a[\"join\"].as_str().unwrap_or(\"\")} "
                    }

                    " - {r._source.title}"
                }
                div {
                    "{r._source.styles.join(\", \")}"
                }
                div {
                    for t in r._source.tracklist {
                        div {
                            class: "p-4",
                            onclick: move |event| log::info!("{event:?}"),
                            "{t[\"position\"].as_str().unwrap_or(\"\")} - {t[\"title\"].as_str().unwrap()}"
                        }
                    }
                }
            }
            if embed != "" {
                iframe {
                    src: "https://www.youtube.com/embed/?playlist={embed}&version=3&fs=0&modestbranding=1&enablejsapi=0",
                    class: "w-full h-full aspect-video"
                }
            }
        }
    }
}
