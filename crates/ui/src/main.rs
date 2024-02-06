#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_web::Config;
use log::LevelFilter;

fn main() {
    // Init debug
    console_error_panic_hook::set_once();

    // now rehydrate
    dioxus_web::launch::launch(|| rsx!(Router::<Route> {}), vec![], Config::new());
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
    render! {
        Link { to: Route::Home {}, "Go to counter" }
        "Blog post {id}"
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
