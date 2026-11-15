//! The bundled Simplified Chinese font, shared by chrome and web content on Windows.

use std::rc::Rc;

pub(crate) const BYTES: &[u8] = include_bytes!("../../resources/fonts/NotoSansSC.ttf");
pub(super) const URL: &str = "https://retsurf-fonts.invalid/NotoSansSC.ttf";

pub(super) fn stylesheet() -> Rc<servo::user_contents::UserStyleSheet> {
    let css = format!(
        r#"
        @font-face {{
            font-family: "retsurf CJK";
            src: url("{URL}") format("truetype");
            font-weight: 100 900;
            font-display: swap;
            unicode-range: U+2E80-2FFF, U+3000-303F, U+3100-312F, U+31A0-31EF,
                           U+3400-4DBF, U+4E00-9FFF, U+F900-FAFF, U+FE30-FE4F,
                           U+FF00-FFEF, U+20000-2FFFF, U+30000-323AF;
        }}
        :root, body:lang(zh) {{
            font-family: "retsurf CJK", serif;
        }}
        "#
    );
    let url = url::Url::parse("https://retsurf-fonts.invalid/cjk.css").expect("literal URL");
    Rc::new(servo::user_contents::UserStyleSheet::new(css, url))
}
