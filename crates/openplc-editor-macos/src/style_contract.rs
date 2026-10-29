#[cfg(test)]
mod tests {
    const CSS: &str = include_str!("../assets/openplc.css");

    #[test]
    fn compiled_css_preserves_pinned_theme_and_window_contracts() {
        for required in [
            ":root{--primary-default:#0464fb",
            "--oplc-title-bar-height:2rem",
            ".font-caption{font-family:Inter,sans-serif}",
            ".font-display{font-family:Poppins,sans-serif}",
            ".dark\\:bg-neutral-950:is(.dark *)",
            ".focus\\:border-brand:focus{border-color:var(--primary-default)}",
            ".nineties,.nineties *{font-family:Tahoma",
            "[data-radix-popper-content-wrapper],[data-radix-select-content],[role=listbox]{z-index:1000!important}",
        ] {
            assert!(CSS.contains(required), "missing pinned CSS contract: {required}");
        }
    }

    #[test]
    fn compiled_css_contains_rust_editor_layout_utilities() {
        for required in [
            ".grid-cols-\\[112px_minmax\\(0\\2c 1fr\\)\\]{grid-template-columns:112px minmax(0,1fr)}",
            ".min-w-\\[760px\\]{min-width:760px}",
            ".dark\\:bg-neutral-900\\/50:is(.dark *){background-color:#2e303880}",
            ".text-\\[9px\\]{font-size:9px}",
            ".rounded-xl{border-radius:.75rem}",
            ".overflow-y-auto{overflow-y:auto}",
            ".disabled\\:opacity-40:disabled{opacity:.4}",
        ] {
            assert!(CSS.contains(required), "missing Rust UI utility from compiled CSS: {required}");
        }
    }
}
