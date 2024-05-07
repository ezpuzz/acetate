#![allow(non_snake_case)]

use std::{collections::HashSet, time::Duration};

use dioxus::prelude::*;
use dioxus_web::Config;
use log::LevelFilter;

use reqwest::StatusCode;
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
    id: i64,
    title: String,
    artists: Vec<Value>,
    styles: Option<Vec<String>>,
    labels: Option<Vec<Value>>,
    tracklist: Vec<Value>,
    videos: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
struct FilterValue {
    count: i64,
    label: String,
}

#[derive(Debug, Clone)]
struct Filter {
    name: String,
    field: String,
    values: Vec<FilterValue>,
}

async fn get_filters() -> Result<Vec<Filter>, reqwest::Error> {
    let resp = reqwest::Client::new()
        .get("http://localhost:3000/filters")
        .send()
        .await?
        .json::<Value>()
        .await?;

    Ok(resp["aggregations"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| Filter {
            name: key.to_owned(),
            field: value["meta"]["field"].as_str().unwrap().to_string(),
            values: value["buckets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| FilterValue {
                    count: v["doc_count"].as_i64().unwrap(),
                    label: v["key"].as_str().unwrap().to_string(),
                })
                .collect(),
        })
        .collect::<Vec<Filter>>())
}

async fn get_releases(
    filters: Vec<Filter>,
    page: i32,
) -> Result<Vec<ElasticResult>, reqwest::Error> {
    let mut query: Vec<(String, String)> = filters
        .iter()
        .flat_map(|f| {
            vec![
                ("field".to_owned(), f.field.to_owned()),
                ("value".to_owned(), f.values[0].label.to_owned()),
            ]
        })
        .collect();

    query.push(("from".to_owned(), (page * 10).to_string()));

    let resp = reqwest::Client::new()
        .get("http://localhost:3000/releases")
        .query(&query)
        .send()
        .await?
        .json::<Value>()
        .await?;

    let hits = &resp["hits"]["hits"];

    let records: Vec<ElasticResult> = serde_json::from_value(hits.clone()).unwrap();

    Ok(records)
}

#[derive(Clone, Debug, PartialEq, Routable)]
enum Route {
    #[route("/")]
    Home,
}

#[component]
fn FilterCheckbox(v: FilterValue, field: String, selected: Signal<Vec<Filter>>) -> Element {
    let f = v.clone();

    rsx! {
        input {
            r#type: "checkbox",
            id: "{v.label}",
            value: "{v.label}",
            onchange: move |evt| {
                if evt.data.value() == "true" {
                    log::info!("Adding filter on {}", field);
                    selected.push(Filter {
                        values: vec![f.to_owned()],
                        name: "".to_string(),
                        field: field.to_owned()
                    });
                } else {
                    selected.remove(selected.iter().position(|filter| *filter.values == [f.to_owned()]  && *filter.field == field).unwrap());
                }
            },
        }
        label {
            r#for: "{v.label}",
            " {v.label}"
        }
    }
}

#[component]
fn Filters(selected_filters: Signal<Vec<Filter>>) -> Element {
    let filters = use_resource(get_filters);

    match filters.read().as_ref() {
        Some(Ok(list)) => {
            // let fs = list.iter().map(|f| Filter {
            //     name: f.name.clone(),
            //     field: "asdf".to_string(),
            //     values: vec![],
            // });

            rsx! {
                for f in list {
                        fieldset {
                            class: "flex flex-none gap-2",
                            legend { "{f.name}" }
                            for vals in f.values.iter().take(200).collect::<Vec<_>>().chunks(20) {
                                div {
                                    class: "flex-none",
                                    for v in vals {
                                        div {
                                            FilterCheckbox{
                                                v: v.to_owned().clone(),
                                                field: f.field.to_owned(),
                                                selected: selected_filters
                                            }
                                        }
                                    }
                                }
                            }
                        }
                },
            }
        }
        Some(Err(e)) => {
            rsx! { "{e:?}"}
        }
        None => {
            rsx! { "loading" }
        }
    }
}

#[component]
fn Home() -> Element {
    let mut selected_filters = use_signal(|| Vec::new() as Vec<Filter>);
    let mut page = use_signal(|| 0);
    let refresh = use_signal(|| true);

    let mut releases = use_resource(move || async move {
        if *refresh.read() {
            let resp = get_releases(
                selected_filters.read().clone().into_iter().collect(),
                *page.read(),
            )
            .await;

            let rsx = match resp {
                Ok(list) => {
                    log::info!("rendering releases");

                    rsx! {
                        for r in list {
                            RecordDisplay { key: "{r._id}", r, refresh }
                        }
                    }
                }
                Err(e) => rsx! { "{e.to_string()}" },
            };
            return rsx;
        }

        rsx! { "Unknown" }
    });

    let release_list = releases().unwrap_or(rsx! {"loading"});

    let mut label_input = use_signal(String::new);
    let mut last_keypress = use_signal(|| 0.);
    let mut run_handler = use_signal(|| false);

    let input_handler = move |evt: Event<FormData>| async move {
        log::info!("input");
        *last_keypress.write() = js_sys::Date::now();
        *label_input.write() = evt.data.value();
    };

    let _ = use_resource(move || async move {
        if run_handler() && last_keypress() > 0. && js_sys::Date::now() - last_keypress() > 100. {
            *run_handler.write() = false;
            *last_keypress.write() = 0.;

            let label = label_input();

            let filter = Filter {
                name: "label".to_string(),
                field: "nested:labels.name".to_string(),
                values: vec![FilterValue {
                    count: 0,
                    label: label.clone(),
                }],
            };

            if let Some(index) = selected_filters
                .iter()
                .position(|f| f.field == filter.field)
            {
                selected_filters.swap_remove(index);
            }
            if !label_input().is_empty() {
                selected_filters.push(filter);
            }

            log::info!("value: {}", label);
        } else {
            async_std::task::sleep(Duration::from_millis(500)).await;
            if !run_handler() && last_keypress() > 0. {
                log::info!("slept");

                *run_handler.write() = true;
            }
        }
    });

    rsx! {
        button {
            onclick: move |_| { releases.restart(); },
            "refresh"
        }
        div {
            class: "flex gap-4",
            Filters { selected_filters: selected_filters }
        }

        div {
            class: "p-2",
            label {
                r#for: "label",
                "Label: "
            }
            input {
                name: "label",
                class: "p-2 border",
                oninput: input_handler
            }
        }

        {release_list}

        button {
            onclick: move |_| { *page.write() += 1 },
            "Next Page"
        }
    }
    // rsx! {
    //     Link { to: Route::Home {}, "Go to counter" }
    //     "Blog post {id}"
    // }
}

#[component]
fn RecordDisplay(r: ElasticResult, refresh: Signal<bool>) -> Element {
    let mut fix_yt = use_signal(|| false);

    let embed = use_memo(move || {
        r._source
            .videos
            .clone()
            .unwrap_or(vec![])
            .iter()
            .map(|url| &url[32..])
            // HashSet makes it unique values only
            .collect::<HashSet<&str>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",")
    });

    let yt_api_url = use_memo(move || {
        format!(
            "https://youtube.googleapis.com/youtube/v3/videos?id={}&key=",
            embed
        )
    });

    let fixed_yts = use_resource(move || async move {
        if *fix_yt.read() {
            let data = reqwest::Client::new()
                .get(yt_api_url())
                .send()
                .await
                .unwrap();
            let valid = data.json::<Value>().await.unwrap()["items"]
                .as_array()
                .unwrap_or(&vec![])
                .to_owned();
            valid
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>()
                .join(",")
        } else {
            embed()
        }
    });

    rsx! {
        div {
            class: "flex py-4",
            div {
                class: "p-4 w-1/2",
                div {
                    a {
                        href: "https://discogs.com/release/{r._source.id}",
                        target: "_blank",
                        "{r._source.id}"
                    }
                    button {
                        class: "ml-4 border rounded p-1 hover:bg-slate-400",
                        onclick: move |_| {
                            async move {
                                // TODO: check if was a 200?
                                let req = reqwest::Client::new()
                                    .post("http://localhost:3000/actions")
                                    .query(&[("action", "hide"), ("identifier", &r._source.id.to_string())])
                                    .send()
                                    .await
                                    .unwrap();

                                assert!(req.status() == StatusCode::OK);

                                *refresh.write() = true;
                            }
                        },
                        "X"
                    }
                    button {
                        class: "ml-4 border rounded p-1 hover:bg-slate-400",
                        onclick: move |_| {
                            async move {
                                // TODO: check if was a 200?
                                let req = reqwest::Client::new()
                                    .post("http://discogs.com/api")
                                    .query(&[("action", "wishlist"), ("identifier", &r._source.id.to_string())])
                                    .send()
                                    .await
                                    .unwrap();

                                assert!(req.status() == StatusCode::OK);

                                // *refresh.write() = true;
                            }
                        },
                        "Wishlist"
                    }
                }
                div {
                    for a in r._source.artists.iter() {
                        "{a[\"name\"].as_str().unwrap()}"
                        " {a[\"join\"].as_str().unwrap_or(\"\")} "
                    }

                    " - {r._source.title}"
                }
                div {
                    "{r._source.styles.clone().unwrap_or(vec![]).join(\", \")}"
                }
                div {
                    "{r._source.labels.clone().unwrap_or(vec![]).iter().map(|l| l[\"name\"].as_str().unwrap()).collect::<Vec<_>>().join(\", \")}"
                }
                div {
                    for t in r._source.tracklist.iter() {
                        div {
                            class: "p-2",
                            "{t[\"position\"].as_str().unwrap_or(\"\")} - {t[\"title\"].as_str().unwrap_or(\"\")}"
                        }
                    }
                }
            }
            if let Some(videos) = fixed_yts.read().as_ref() {
                if !videos.is_empty() {
                    button {
                        class: "border rounded p-2",
                        onclick: move |_| async move {
                            log::info!("fixing youtubes");
                            *fix_yt.write() = true;
                        },
                        "fix"
                    }
                    iframe {
                        src: "https://www.youtube.com/embed/?playlist={videos}&version=3&fs=0&modestbranding=1&enablejsapi=0&rel=0",
                        height: "1920px",
                        width: "1080px",
                        class: "w-full h-full aspect-video"
                    }
                }
            }
        }
    }
}
