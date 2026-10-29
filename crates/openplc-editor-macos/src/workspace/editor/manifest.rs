use dioxus::prelude::*;

use super::monaco::ManifestMonaco;

#[component]
pub fn ManifestEditor() -> Element {
    rsx! {
        ManifestMonaco {}
    }
}
