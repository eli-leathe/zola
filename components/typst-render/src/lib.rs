//! Native Typst HTML rendering. The project root is the Typst filesystem root.
use errors as anyhow;
use errors::{Context, Result, bail};
use regex::Regex;
use std::path::{Path, PathBuf};
use typst::diag::{FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration, Label, Repr, Selector};
use typst::introspection::Introspector;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Features, Library, LibraryExt, World};
use typst_html::{HtmlDocument, HtmlOptions, html};
use typst_kit::datetime::Time;
use typst_kit::diagnostics::{DiagnosticFormat, DiagnosticWorld, emit, termcolor::NoColor};
use typst_kit::downloader::SystemDownloader;
use typst_kit::files::{FileStore, FsRoot, SystemFiles};
use typst_kit::fonts::{self, FontStore};
use typst_kit::packages::SystemPackages;

pub struct Typst {
    root: PathBuf,
    library: LazyHash<Library>,
    fonts: FontStore,
    files: FileStore<SystemFiles>,
    time: Time,
}

pub struct Compiled {
    pub metadata: serde_json::Value,
    pub rendered: Rendered,
}

pub struct Rendered {
    pub body: String,
    pub head: String,
}

struct NoteWorld<'a> {
    typst: &'a Typst,
    main: Source,
}

impl Typst {
    pub fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let packages = SystemPackages::new(SystemDownloader::new("zola"));
        let files = FileStore::new(SystemFiles::new(FsRoot::new(root.clone()), packages));
        let features: Features = [Feature::Html].into_iter().collect();
        let library = LazyHash::new(Library::builder().with_features(features).build());
        let mut fonts = FontStore::new();
        fonts.extend(fonts::system());
        fonts.extend(fonts::embedded());
        Ok(Self { root, library, fonts, files, time: Time::system() })
    }

    pub fn compile(
        &self,
        path: &Path,
        prelude: Option<&str>,
        metadata_label: &str,
    ) -> Result<Compiled> {
        let path = path.canonicalize()?;
        let relative = path.strip_prefix(&self.root)?.to_str().context("non-UTF-8 source path")?;
        let wrapper = source_wrapper(relative, prelude);
        let id = FileId::unique(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("/__zola_wrapper.typ")?,
        ));
        let world = NoteWorld { typst: self, main: Source::new(id, wrapper) };
        let compilation = typst::compile::<HtmlDocument>(&world);
        let warnings: Vec<_> = compilation
            .warnings
            .into_iter()
            .filter(|warning| {
                warning.message != "html export is under active development and incomplete"
                    || !warning.span.is_detached()
            })
            .collect();
        if !warnings.is_empty() {
            eprint!("{}", diagnostics(&world, &warnings));
        }
        let document = compilation
            .output
            .map_err(|errors| anyhow::anyhow!("{}", diagnostics(&world, &errors)))?;

        let label = Label::construct(metadata_label.into()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let selector = Selector::Label(label);
        let mut matches = document.introspector().query(&selector);
        if matches.len() != 1 {
            bail!("Expected exactly one <{metadata_label}> metadata element in {}", path.display());
        }
        let value =
            matches.pop().unwrap().get_by_name("value").map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let metadata = serde_json::to_value(value)?;
        let exported = html(&document, &HtmlOptions::default())
            .map_err(|errors| anyhow::anyhow!("{}", diagnostics(&world, &errors)))?;
        Ok(Compiled { metadata, rendered: extract_html(&exported, &path)? })
    }
}

fn source_wrapper(relative: &str, prelude: Option<&str>) -> String {
    let mut wrapper = String::new();
    if let Some(prelude) = prelude {
        wrapper.push_str(&format!("#import {}: prelude\n#show: prelude\n", prelude.repr()));
    }
    let relative = relative.replace(std::path::MAIN_SEPARATOR, "/");
    wrapper.push_str(&format!("#include {}\n", format!("/{relative}").as_str().repr()));
    wrapper
}

fn diagnostics(world: &NoteWorld<'_>, errors: &[SourceDiagnostic]) -> String {
    let mut buffer = NoColor::new(Vec::new());
    match emit(&mut buffer, world, errors, DiagnosticFormat::Human) {
        Ok(()) => String::from_utf8_lossy(&buffer.into_inner()).into_owned(),
        Err(_) => errors.iter().map(|error| error.message.as_str()).collect::<Vec<_>>().join("; "),
    }
}

fn extract_html(html: &str, path: &Path) -> Result<Rendered> {
    let body = Regex::new(r"(?is)<body(?:\s[^>]*)?>(.*?)</body>")?;
    let head = Regex::new(r"(?is)<head(?:\s[^>]*)?>(.*?)</head>")?;
    let (Some(body), Some(head)) = (body.captures(html), head.captures(html)) else {
        bail!("No HTML head or body produced for {}", path.display());
    };
    let style = Regex::new(r"(?is)<style\b[^>]*>.*?</style>")?;
    let styles = style.find_iter(&head[1]).map(|m| m.as_str()).collect::<Vec<_>>().join("\n");
    Ok(Rendered { body: body[1].to_owned(), head: styles })
}

#[cfg(test)]
mod tests {
    use super::{extract_html, source_wrapper};
    use std::path::Path;

    #[test]
    fn extracts_body_and_only_generated_styles() {
        let rendered = extract_html(
            "<html><head><title>Ignored</title><style>.a { color: red }</style><meta name='x'><style>.b {}</style></head><body class='typst'><p>Hello</p><svg></svg></body></html>",
            Path::new("note.typ"),
        ).unwrap();
        assert_eq!(rendered.body, "<p>Hello</p><svg></svg>");
        assert_eq!(rendered.head, "<style>.a { color: red }</style>\n<style>.b {}</style>");
    }

    #[test]
    fn wrapper_quotes_paths_as_typst_strings() {
        let wrapper = source_wrapper("content/quote\"\\\n\u{1}.typ", Some("/content/prelude.typ"));
        assert!(wrapper.starts_with("#import \"/content/prelude.typ\": prelude\n#show: prelude\n"));
        assert!(typst::syntax::parse(&wrapper).errors_and_warnings().0.is_empty());
        assert_eq!(source_wrapper("content/note.typ", None), "#include \"/content/note.typ\"\n");
    }

    #[test]
    fn rejects_incomplete_documents() {
        for html in ["<p>fragment</p>", "<head></head>", "<body></body>"] {
            assert!(extract_html(html, Path::new("note.typ")).is_err());
        }
    }
}

impl DiagnosticWorld for NoteWorld<'_> {
    fn name(&self, id: FileId) -> String {
        if id == self.main.id() {
            "<zola-wrapper>".into()
        } else {
            self.typst
                .files
                .loader()
                .resolve(id)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| id.vpath().get_with_slash().to_owned())
        }
    }
}

impl World for NoteWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.typst.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.typst.fonts.book()
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() { Ok(self.main.clone()) } else { self.typst.files.source(id) }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() {
            Ok(Bytes::from_string(self.main.text().to_owned()))
        } else {
            self.typst.files.file(id)
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.typst.fonts.font(index)
    }
    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.typst.time.today(offset)
    }
}
