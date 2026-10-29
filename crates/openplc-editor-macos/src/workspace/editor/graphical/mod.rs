mod fbd;
mod ladder;
mod node;

use dioxus::prelude::*;

use crate::plc_model::PouLanguage;

use fbd::FbdEditor;
use ladder::LadderEditor;

#[component]
pub fn GraphicalEditor(language: PouLanguage, pou_name: String) -> Element {
    rsx! {
        div { class: "relative h-full w-full overflow-y-auto",
            div { class: "h-full w-full",
                match language {
                    PouLanguage::Ld => rsx! { LadderEditor { pou_name } },
                    PouLanguage::Fbd => rsx! { FbdEditor { pou_name } },
                    _ => VNode::empty(),
                }
            }
        }
    }
}
