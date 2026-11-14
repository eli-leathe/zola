//! Connect Typst's embedded raster output to Zola's normal image queue.
use std::sync::LazyLock;

use base64::{Engine, engine::general_purpose::STANDARD};
use config::TypstImages;
use errors::{Context, Result, ensure};
use imageproc::{Processor, ResizeOperation};
use regex::Regex;

// Typst's HTML exporter quotes attributes with double quotes. Only img sources
// are rewritten: inline SVG diagrams, links and other data URLs stay untouched.
static RASTER_SOURCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)(<img\b[^>]*?\s)src="data:image/(?:png|jpeg|webp);base64,([^"]*)""#).unwrap()
});

pub(super) fn process(
    html: &str,
    processor: &mut Processor,
    settings: &TypstImages,
) -> Result<String> {
    if !settings.enabled {
        return Ok(html.to_owned());
    }
    ensure!(
        settings.max_width > 0 && settings.max_height > 0,
        "Typst image maximum dimensions must be positive"
    );
    rewrite_rasters(html, |bytes| {
        // Zola's normal encoder pipeline handles still images. Do not silently
        // turn animated PNG/WebP images into their first frame.
        if imageproc::is_animated(&bytes)? {
            return Ok(None);
        }
        let response = processor.enqueue_bytes(
            ResizeOperation::Fit(settings.max_width, settings.max_height),
            bytes,
            &settings.format,
            settings.quality,
            None,
            "lanczos3",
        )?;
        Ok(Some(response.url))
    })
}

fn rewrite_rasters(
    html: &str,
    mut enqueue: impl FnMut(Vec<u8>) -> Result<Option<String>>,
) -> Result<String> {
    let mut output = String::with_capacity(html.len());
    let mut end = 0;
    for found in RASTER_SOURCE.captures_iter(html) {
        let whole = found.get(0).unwrap();
        output.push_str(&html[end..whole.start()]);
        let bytes = STANDARD.decode(&found[2]).context("Invalid embedded Typst raster image")?;
        match enqueue(bytes)? {
            Some(url) => {
                // base_url can contain HTML-sensitive characters. Preserve a
                // valid quoted attribute just as Zola's template escaping does.
                let url = url
                    .replace('&', "&amp;")
                    .replace('"', "&quot;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                output.push_str(&format!("{}src=\"{}\"", &found[1], url));
            }
            None => output.push_str(whole.as_str()),
        }
        end = whole.end();
    }
    output.push_str(&html[end..]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::rewrite_rasters;

    #[test]
    fn rewrites_only_raster_img_sources_and_preserves_attributes() {
        let html = r#"<img alt="A" src="data:image/png;base64,YWJj" width="10"><svg><image href="data:image/png;base64,YWJj"/></svg><a href="data:image/png;base64,YWJj">link</a><img src="/photo.jpg"><img src="data:image/svg+xml;base64,YWJj"><img data-src="data:image/png;base64,YWJj">"#;
        let mut calls = 0;
        let output = rewrite_rasters(html, |bytes| {
            assert_eq!(bytes, b"abc");
            calls += 1;
            Ok(Some("https://example.org/processed_images/a.webp".into()))
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(
            output,
            html.replacen(
                "src=\"data:image/png;base64,YWJj\"",
                "src=\"https://example.org/processed_images/a.webp\"",
                1
            )
        );
    }

    #[test]
    fn repeated_images_get_the_same_asset_url() {
        let html =
            r#"<img src="data:image/png;base64,YWJj"><img src="data:image/jpeg;base64,YWJj">"#;
        let output =
            rewrite_rasters(html, |_| Ok(Some("/processed_images/shared.webp".into()))).unwrap();
        assert_eq!(output.matches("/processed_images/shared.webp").count(), 2);
        assert!(!output.contains("data:image"));
    }

    #[test]
    fn callback_can_preserve_animations_and_errors_propagate() {
        let html = r#"<img src="data:image/webp;base64,YWJj">"#;
        assert_eq!(rewrite_rasters(html, |_| Ok(None)).unwrap(), html);
        assert!(rewrite_rasters(html, |_| errors::bail!("invalid image")).is_err());
        assert!(
            rewrite_rasters(r#"<img src="data:image/png;base64,invalid!">"#, |_| Ok(None)).is_err()
        );
    }

    #[test]
    fn disabled_processing_and_invalid_dimensions_do_not_touch_the_filesystem() {
        let mut processor = imageproc::Processor::new(
            std::path::PathBuf::new(),
            &config::Config::default_for_test(),
        );
        let mut settings = config::TypstImages::default();
        settings.enabled = false;
        let html = r#"<img src="data:image/png;base64,invalid!">"#;
        assert_eq!(super::process(html, &mut processor, &settings).unwrap(), html);
        assert_eq!(processor.num_img_ops(), 0);
        settings.enabled = true;
        settings.max_width = 0;
        assert!(super::process("", &mut processor, &settings).is_err());
        assert_eq!(processor.num_img_ops(), 0);
    }

    #[test]
    fn generated_urls_are_attribute_escaped() {
        let output = rewrite_rasters(r#"<img src="data:image/png;base64,YWJj">"#, |_| {
            Ok(Some("/a?x=1&y=\"<>".into()))
        })
        .unwrap();
        assert!(output.contains("src=\"/a?x=1&amp;y=&quot;&lt;&gt;\""));
    }
}
