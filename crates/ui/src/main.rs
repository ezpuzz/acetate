#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_web::Config;
use log::LevelFilter;

fn main() {
    // Init debug
    console_error_panic_hook::set_once();
    dioxus_logger::init(LevelFilter::Info).expect("failed to init logger");

    // now rehydrate
    dioxus_web::launch::launch(|| rsx!(Router::<Route> {}), vec![], Config::new());
}

struct Record {
    id: String,
}

async fn get_releases() -> Result<Vec<Record>, reqwest::Error> {
    let search = elasticsearch_dsl::Search::new().size(10);
    let resp: serde_json::Value = reqwest::Client::new()
        .post("https://localhost:9200/releases")
        .json(&serde_json::to_string(&search).unwrap())
        .send()
        .await?
        .json()
        .await?;
    dbg!(search);

    println!("{resp:#?}");
    Ok(vec![Record { id: "1".into() }])
}

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
    #[route("/blog/:id")]
    Blog { id: i32 },
}

#[component]
fn Blog(id: i32) -> Element {
    let releases = use_resource(move || get_releases());

    match releases.read().as_ref() {
        Some(Ok(list)) => {
            rsx! {
                div {
                    for r in list {
                        Record { id: r.id.clone() }
                    }
                }
            }
        }
        Some(Err(_)) => rsx! { "Error"},
        None => rsx! {"Loading"},
    }
    // rsx! {
    //     Link { to: Route::Home {}, "Go to counter" }
    //     "Blog post {id}"
    // }
}

#[component]
fn Record(id: String) -> Element {
    rsx! {
        div {
            id
        }
    }
}

#[component]
fn Home() -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        Link {
            to: Route::Blog {
                id: count()
            },
            "Go to blog"
        }
        div {
            h1 { "High-Five counter: {count}" }
            button { onclick: move |_| count += 1, "Up high!" }
            button { onclick: move |_| count -= 1, "Down low!" }
        }
    }
}
