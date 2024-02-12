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
    id: i64,
    title: String,
    artists: Vec<Value>,
    styles: Option<Vec<String>>,
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
        .map(|(key, value)| {
            log::info!("{}", key);
            Filter {
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
            }
        })
        .collect::<Vec<Filter>>())
}

async fn get_releases(
    filters: Vec<Filter>,
    page: i32,
) -> Result<Vec<ElasticResult>, reqwest::Error> {
    log::info!("{:?}", filters);

    let mut query: Vec<(String, String)> = filters
        .iter()
        .map(|f| {
            vec![
                ("field".to_owned(), f.field.to_owned()),
                ("value".to_owned(), f.values[0].label.to_owned()),
            ]
        })
        .flatten()
        .collect();

    query.push(("from".to_owned(), (page * 10).to_string()));

    log::info!("{:?}", query);

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

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home { r: ElasticResult },
}

#[component]
fn FilterCheckbox(v: FilterValue, field: String, selected: Signal<Vec<Filter>>) -> Element {
    let mut checked = use_signal(|| false);

    use_effect(move || log::info!("{:?}", checked));

    let f = v.clone();

    rsx! {
        input {
            r#type: "checkbox",
            id: "{v.label}",
            value: "{v.label}",
            onchange: move |evt| {
                *checked.write() = evt.data.value() == "true";
                selected.push(Filter {
                    values: vec![f.to_owned()],
                    name: "".to_string(),
                    field: field.to_owned()
                });
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
    let mut filters = use_resource(get_filters);

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
fn Releases() -> Element {
    rsx! {}
}

#[component]
fn Home(r: ElasticResult) -> Element {
    let mut selected_filters = use_signal(|| Vec::new() as Vec<Filter>);
    let mut page = use_signal(|| 0);
    let mut releases = use_resource(move || {
        get_releases(
            selected_filters.read().iter().cloned().collect(),
            *page.read(),
        )
    });

    rsx! {
        div {
            class: "flex gap-4",
            Filters { selected_filters: selected_filters }
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
                                reqwest::Client::new()
                                    .post("http://localhost:3000/actions")
                                    .query(&[("action", "hide"), ("identifier", &r._source.id.to_string())])
                                    .send()
                                    .await
                                    .unwrap();
                            }
                        },
                        "X"
                    }
                }
                div {
                    for a in r._source.artists {
                        "{a[\"name\"].as_str().unwrap()}"
                        " {a[\"join\"].as_str().unwrap_or(\"\")} "
                    }

                    " - {r._source.title}"
                }
                div {
                    "{r._source.styles.unwrap_or(vec![]).join(\", \")}"
                }
                div {
                    for t in r._source.tracklist {
                        div {
                            class: "p-2",
                            onclick: move |event| log::info!("{event:?}"),
                            "{t[\"position\"].as_str().unwrap_or(\"\")} - {t[\"title\"].as_str().unwrap_or(\"\")}"
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
