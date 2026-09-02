// ────────────────────────────────────────────────────────────────
// EPUB 3 assembly
//
// EPUB is the only mainstream ebook format that carries audio: `<audio>` in the content documents,
// plus Media Overlays (SMIL) to read along with a narration. That is the whole reason this is EPUB and
// not PDF — a poetic edition of a song that cannot play the song is missing the point.
//
// An EPUB is a zip with three rules the readers actually enforce:
//
//   1. The first entry must be `mimetype`, STORED (not deflated), with no extra field. Readers check
//      the bytes at a fixed offset; a compressed mimetype fails to open with no useful message.
//   2. `META-INF/container.xml` points at the package document.
//   3. The package document (`.opf`) lists every file as a manifest item and orders the readable ones
//      in a spine. A file present in the zip but missing from the manifest simply does not exist as
//      far as the reader is concerned — the commonest way a hand-built EPUB loses its images.
//
// Everything here writes entries STORED. The payload is JPEG/PNG/MP3 (already compressed) plus a few
// kilobytes of XHTML, so deflate would buy almost nothing and cost a dependency.
// ────────────────────────────────────────────────────────────────

/// CRC-32 (IEEE), which every zip entry header carries.
///
/// Implemented rather than pulled in: it is fifteen lines, and a wrong checksum makes an EPUB that
/// opens in the lenient readers and fails validation in the ones the stores run.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

struct Entry {
    name: String,
    offset: u32,
    crc: u32,
    size: u32,
}

/// A store-only zip writer, enough for EPUB and nothing more.
pub struct Zip {
    buf: Vec<u8>,
    entries: Vec<Entry>,
}

impl Zip {
    pub fn new() -> Self {
        Zip { buf: Vec::new(), entries: Vec::new() }
    }

    /// Append one file. Order matters: `mimetype` must be added first.
    pub fn add(&mut self, name: &str, data: &[u8]) {
        let offset = self.buf.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_bytes = name.as_bytes();

        self.buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());   // local file header
        self.buf.extend_from_slice(&10u16.to_le_bytes());            // version needed
        self.buf.extend_from_slice(&0u16.to_le_bytes());             // flags
        self.buf.extend_from_slice(&0u16.to_le_bytes());             // method 0 = stored
        self.buf.extend_from_slice(&0u16.to_le_bytes());             // mod time
        self.buf.extend_from_slice(&0u16.to_le_bytes());             // mod date
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());             // compressed == uncompressed
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());             // no extra field
        self.buf.extend_from_slice(name_bytes);
        self.buf.extend_from_slice(data);

        self.entries.push(Entry { name: name.to_string(), offset, crc, size });
    }

    /// Close the archive and return its bytes.
    pub fn finish(mut self) -> Vec<u8> {
        let dir_start = self.buf.len() as u32;
        for e in &self.entries {
            let name_bytes = e.name.as_bytes();
            self.buf.extend_from_slice(&0x0201_4b50u32.to_le_bytes());  // central directory header
            self.buf.extend_from_slice(&20u16.to_le_bytes());           // version made by
            self.buf.extend_from_slice(&10u16.to_le_bytes());           // version needed
            self.buf.extend_from_slice(&0u16.to_le_bytes());            // flags
            self.buf.extend_from_slice(&0u16.to_le_bytes());            // stored
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&e.crc.to_le_bytes());
            self.buf.extend_from_slice(&e.size.to_le_bytes());
            self.buf.extend_from_slice(&e.size.to_le_bytes());
            self.buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());            // extra len
            self.buf.extend_from_slice(&0u16.to_le_bytes());            // comment len
            self.buf.extend_from_slice(&0u16.to_le_bytes());            // disk number
            self.buf.extend_from_slice(&0u16.to_le_bytes());            // internal attrs
            self.buf.extend_from_slice(&0u32.to_le_bytes());            // external attrs
            self.buf.extend_from_slice(&e.offset.to_le_bytes());
            self.buf.extend_from_slice(name_bytes);
        }
        let dir_size = self.buf.len() as u32 - dir_start;
        let n = self.entries.len() as u16;
        self.buf.extend_from_slice(&0x0605_4b50u32.to_le_bytes());      // end of central directory
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&n.to_le_bytes());
        self.buf.extend_from_slice(&n.to_le_bytes());
        self.buf.extend_from_slice(&dir_size.to_le_bytes());
        self.buf.extend_from_slice(&dir_start.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());                // comment len
        self.buf
    }
}

/// Escape text for XML content and attributes.
///
/// Scripture and titles contain ampersands and quotes, and an unescaped `&` makes the whole document
/// unparseable — which a reader reports as a corrupt book, not as a bad character.
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// What a page *is*, which decides how it is set and what `epub:type` it declares.
///
/// A book is not a stack of identical pages. A half-title, a copyright page and a part divider are
/// typographically nothing like a chapter, and a reader that is told which is which can do the
/// right thing with each — skip the front matter when resuming, list the parts in its own contents,
/// read the copyright page in a different voice. Stores check for several of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// An ordinary page of an edition.
    Body,
    /// The title page: title, subtitle, author, imprint.
    TitlePage,
    /// Copyright, rights and identifiers.
    Copyright,
    /// Dedication, epigraph, foreword, preface — set centred and quiet.
    FrontMatter,
    /// A part divider: a title alone on a page.
    Part,
    /// Afterword, about the author, also-by, colophon.
    BackMatter,
}

impl Role {
    /// The `epub:type` a reading system understands. Structural semantics rather than decoration:
    /// this is what lets a reader open "the first page of the actual book" rather than the
    /// copyright notice.
    pub fn epub_type(self) -> &'static str {
        match self {
            Role::Body => "chapter",
            Role::TitlePage => "titlepage",
            Role::Copyright => "copyright-page",
            Role::FrontMatter => "frontmatter",
            Role::Part => "part",
            Role::BackMatter => "backmatter",
        }
    }
    pub fn css_class(self) -> &'static str {
        match self {
            Role::Body => "body-page",
            Role::TitlePage => "titlepage",
            Role::Copyright => "copyright",
            Role::FrontMatter => "frontmatter",
            Role::Part => "part",
            Role::BackMatter => "backmatter",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "title" | "titlepage" => Role::TitlePage,
            "copyright" => Role::Copyright,
            "front" | "frontmatter" => Role::FrontMatter,
            "part" => Role::Part,
            "back" | "backmatter" => Role::BackMatter,
            _ => Role::Body,
        }
    }
}

/// One page of a graphic-novel edition.
pub struct Page {
    /// File stem, e.g. `page-01`.
    pub id: String,
    pub heading: String,
    /// Paragraphs of poetic text, in order.
    pub lines: Vec<String>,
    /// Image file name inside the book, if the page has art.
    pub image: Option<String>,
    /// Caption under the art.
    pub caption: Option<String>,
    /// A line of dialogue, laid over the art as a real HTML bubble rather than baked into the
    /// picture. See typography.rs for why: the words stay selectable, reachable by a screen reader,
    /// able to reflow — and translatable, so one artwork serves all sixteen languages this app
    /// ships instead of one set of panels per language.
    pub dialogue: Option<String>,
    /// The bubble's shape: speech · thought · shout · caption.
    pub bubble_kind: String,
    /// Where the speaker is, as fractions of the panel. The bubble is placed away from this point,
    /// because a bubble that covers a face is worse than no bubble.
    pub speaker_at: (f64, f64),
    /// This page has a Media Overlay: `<page-id>.smil` narrates it.
    pub has_overlay: bool,
    /// The audio range this page covers, in seconds. Only used when `has_overlay`.
    pub span: (f64, f64),
    /// Which audio file narrates this page, when the book carries more than one.
    ///
    /// A single edition has one song and every overlay points at it. A volume of twelve editions has
    /// twelve, and a page narrated by the wrong one is worse than a page narrated by none — so a
    /// page may name its own, falling back to the book's first file when it does not.
    pub audio: Option<String>,
    /// What this page is. Decides its `epub:type` and how it is set.
    pub role: Role,
    /// Where it sits in the table of contents: 0 not listed at all, 1 top level, 2 nested under the
    /// last top-level entry.
    ///
    /// Zero exists because a contents page listing "Title page, Copyright, Dedication" before the
    /// book starts is the mark of an export rather than of a book, and because a 40-page edition
    /// whose every page is a top-level entry has a contents page nobody can use.
    pub nav_depth: u8,
}

impl Page {
    /// A plain body page — the shape every caller wanted before roles existed.
    pub fn body(id: impl Into<String>, heading: impl Into<String>, lines: Vec<String>) -> Self {
        Page {
            id: id.into(), heading: heading.into(), lines,
            image: None, caption: None, dialogue: None,
            bubble_kind: "speech".into(), speaker_at: (0.5, 0.7),
            has_overlay: false, span: (0.0, 0.0), audio: None,
            role: Role::Body, nav_depth: 1,
        }
    }
}

/// The XHTML for one page.
pub fn page_xhtml(page: &Page, title: &str) -> String {
    let mut body = String::new();
    if !page.heading.is_empty() {
        // One <h1> per book, on the title page. Everywhere else the heading is a level down, so a
        // reading system's outline has a root rather than forty peers.
        let tag = if page.role == Role::TitlePage { "h1" } else { "h2" };
        body.push_str(&format!("    <{tag}>{}</{tag}>\n", xml_escape(&page.heading)));
    }
    if let Some(img) = &page.image {
        body.push_str(&format!(
            "    <figure class=\"panel\">\n      <img src=\"images/{}\" alt=\"{}\"/>\n",
            xml_escape(img), xml_escape(page.caption.as_deref().unwrap_or(&page.heading)),
        ));
        if let Some(line) = page.dialogue.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let kind = crate::typography::Bubble::parse(&page.bubble_kind);
            body.push_str(&format!("      {}\n",
                crate::typography::bubble_html(line, kind, page.speaker_at.0, page.speaker_at.1)));
        }
        if let Some(c) = &page.caption {
            body.push_str(&format!("      <figcaption>{}</figcaption>\n", xml_escape(c)));
        }
        body.push_str("    </figure>\n");
    }
    for line in &page.lines {
        if line.trim().is_empty() { continue; }
        body.push_str(&format!("    <p>{}</p>\n", xml_escape(line)));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{lang}\">\n\
         <head>\n  <title>{title}</title>\n  \
         <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>\n</head>\n\
         <body class=\"{cls}\">\n  <section epub:type=\"{etype}\" id=\"body-{id}\">\n{body}  </section>\n</body>\n</html>\n",
        lang = "en", title = xml_escape(title), body = body, id = xml_escape(&page.id),
        cls = page.role.css_class(), etype = page.role.epub_type(),
    )
}

/// Everything about the book that is not the book.
///
/// The first four fields are all an EPUB is *required* to carry, and for a long time they were all
/// this writer produced. That is enough for a file a reader will open and not enough for a book a
/// store will take: retailers match on ISBN, sort by publisher and pubdate, build category pages
/// from subjects, and group a series by name and number. A book missing those is not rejected — it
/// is accepted and then invisible, which is worse.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub title: String,
    pub subtitle: Option<String>,
    pub author: String,
    /// Everybody else, as (role, name). Roles are MARC relators — `ill` illustrator, `trl`
    /// translator, `edt` editor — because that is what the format and the stores understand.
    pub contributors: Vec<(String, String)>,
    pub language: String,
    /// The `dc:identifier`. A UUID unless there is an ISBN.
    pub book_id: String,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub rights: Option<String>,
    pub subjects: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<i64>,
    /// `dc:date`, ISO-8601. The publication date, not the build date.
    pub pubdate: Option<String>,
}

impl Metadata {
    pub fn new(title: &str, author: &str, language: &str, book_id: &str) -> Self {
        Metadata {
            title: title.into(), author: author.into(),
            language: if language.is_empty() { "en".into() } else { language.into() },
            book_id: book_id.into(),
            ..Default::default()
        }
    }
}

/// The package document: metadata, every file as a manifest item, and the reading order.
///
/// `audio` is the narration or the song itself; when present it is manifested so a reader can play it
/// and, with `overlays`, follow along.
pub fn content_opf(
    meta: &Metadata,
    pages: &[Page],
    images: &[String],
    audio: &[String],
    cover: Option<&str>,
    modified_iso: &str,
) -> String {
    let mut manifest = String::from(
        "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
         \x20   <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n"
    );
    let mut spine = String::new();
    for p in pages {
        // A page with an overlay declares it; a reader without overlay support ignores the attribute.
        let overlay = if p.has_overlay {
            manifest.push_str(&format!(
                "    <item id=\"smil-{id}\" href=\"{id}.smil\" media-type=\"application/smil+xml\"/>\n",
                id = xml_escape(&p.id),
            ));
            format!(" media-overlay=\"smil-{}\"", xml_escape(&p.id))
        } else { String::new() };
        manifest.push_str(&format!(
            "    <item id=\"{id}\" href=\"{id}.xhtml\" media-type=\"application/xhtml+xml\"{overlay}/>\n",
            id = xml_escape(&p.id),
        ));
        spine.push_str(&format!("    <itemref idref=\"{}\"/>\n", xml_escape(&p.id)));
    }
    for (i, img) in images.iter().enumerate() {
        let props = if cover == Some(img.as_str()) { " properties=\"cover-image\"" } else { "" };
        manifest.push_str(&format!(
            "    <item id=\"img{i}\" href=\"images/{}\" media-type=\"{}\"{props}/>\n",
            xml_escape(img), media_type(img),
        ));
    }
    for (i, a) in audio.iter().enumerate() {
        manifest.push_str(&format!(
            "    <item id=\"audio{i}\" href=\"audio/{}\" media-type=\"{}\"/>\n",
            xml_escape(a), media_type(a),
        ));
    }
    let cover_meta = images.iter().position(|i| Some(i.as_str()) == cover)
        .map(|i| format!("    <meta name=\"cover\" content=\"img{i}\"/>\n"))
        .unwrap_or_default();

    // The optional half of the metadata, emitted only where there is something to say. An empty
    // <dc:publisher/> is worse than none: a store reads it as a publisher named "".
    let mut extra = String::new();
    if let Some(sub) = meta.subtitle.as_deref().filter(|s| !s.trim().is_empty()) {
        extra.push_str(&format!(
            "    <meta property=\"title-type\" refines=\"#title\">main</meta>\n\
             \x20   <dc:title id=\"subtitle\">{}</dc:title>\n\
             \x20   <meta property=\"title-type\" refines=\"#subtitle\">subtitle</meta>\n",
            xml_escape(sub)));
    }
    for (i, (role, name)) in meta.contributors.iter().enumerate() {
        if name.trim().is_empty() { continue; }
        extra.push_str(&format!(
            "    <dc:contributor id=\"contrib{i}\">{name}</dc:contributor>\n\
             \x20   <meta property=\"role\" refines=\"#contrib{i}\" scheme=\"marc:relators\">{role}</meta>\n",
            name = xml_escape(name), role = xml_escape(role), i = i));
    }
    if let Some(isbn) = meta.isbn.as_deref().filter(|s| !s.trim().is_empty()) {
        extra.push_str(&format!(
            "    <dc:identifier id=\"isbn\">urn:isbn:{}</dc:identifier>\n", xml_escape(isbn)));
    }
    for (key, value) in [
        ("dc:publisher", meta.publisher.as_deref()),
        ("dc:description", meta.description.as_deref()),
        ("dc:rights", meta.rights.as_deref()),
        ("dc:date", meta.pubdate.as_deref()),
    ] {
        if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
            extra.push_str(&format!("    <{key}>{}</{key}>\n", xml_escape(v)));
        }
    }
    for subject in meta.subjects.iter().filter(|s| !s.trim().is_empty()) {
        extra.push_str(&format!("    <dc:subject>{}</dc:subject>\n", xml_escape(subject)));
    }
    // A series is `belongs-to-collection`, which is what a reading system groups by. The index is
    // emitted only alongside a name, since a book that is number 3 of nothing sorts nowhere.
    if let Some(series) = meta.series.as_deref().filter(|s| !s.trim().is_empty()) {
        extra.push_str(&format!(
            "    <meta property=\"belongs-to-collection\" id=\"series\">{}</meta>\n\
             \x20   <meta property=\"collection-type\" refines=\"#series\">series</meta>\n",
            xml_escape(series)));
        if let Some(n) = meta.series_index {
            extra.push_str(&format!(
                "    <meta property=\"group-position\" refines=\"#series\">{n}</meta>\n"));
        }
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n\
         \x20 <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:opf=\"http://www.idpf.org/2007/opf\">\n\
         \x20   <dc:identifier id=\"bookid\">{id}</dc:identifier>\n\
         \x20   <dc:title id=\"title\">{title}</dc:title>\n\
         \x20   <dc:creator>{author}</dc:creator>\n\
         \x20   <dc:language>{language}</dc:language>\n\
         \x20   <meta property=\"dcterms:modified\">{modified}</meta>\n{extra}{cover_meta}\
         \x20 </metadata>\n\
         \x20 <manifest>\n{manifest}  </manifest>\n\
         \x20 <spine>\n{spine}  </spine>\n\
         </package>\n",
        id = xml_escape(&meta.book_id), title = xml_escape(&meta.title),
        author = xml_escape(&meta.author), language = xml_escape(&meta.language),
        modified = modified_iso, extra = extra,
    )
}

/// A Media Overlay (SMIL) for one page: which audio range narrates it.
///
/// This is the piece that makes an EPUB *read along* rather than merely carry a file. It is worth being
/// honest about the granularity: the timings come from the song's own section analysis, so a page is
/// highlighted for the stretch of audio that section covers — paragraph-level sync, not word-level.
/// Word-level would need forced alignment against the lyrics, which is a different problem entirely.
///
/// A reader that does not support overlays ignores them and still plays the audio, so this can never
/// make a book worse.
pub fn page_smil(page_id: &str, audio_file: &str, start: f64, end: f64) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <smil xmlns=\"http://www.w3.org/ns/SMIL\" xmlns:epub=\"http://www.idpf.org/2007/ops\" version=\"3.0\">\n\
         \x20 <body>\n\
         \x20   <par id=\"par-{page_id}\">\n\
         \x20     <text src=\"{page_id}.xhtml#body-{page_id}\"/>\n\
         \x20     <audio src=\"audio/{audio}\" clipBegin=\"{start:.3}s\" clipEnd=\"{end:.3}s\"/>\n\
         \x20   </par>\n\
         \x20 </body>\n\
         </smil>\n",
        page_id = xml_escape(page_id), audio = xml_escape(audio_file),
        start = start.max(0.0), end = end.max(start + 0.1),
    )
}

/// Split a track into one span per page, from the analysed section timings where there are any.
///
/// Falls back to equal division. That is a guess, and a guess that reads along roughly is better than
/// no read-along at all — but the caller is told which it got, because "roughly" should not be
/// advertised as "synced".
pub fn page_spans(total_seconds: f64, section_starts: &[f64], pages: usize) -> (Vec<(f64, f64)>, bool) {
    if pages == 0 { return (Vec::new(), false); }
    let total = if total_seconds > 0.5 { total_seconds } else { 1.0 };

    if section_starts.len() >= pages && pages > 0 {
        let mut spans = Vec::with_capacity(pages);
        for i in 0..pages {
            let start = section_starts[i].max(0.0);
            let end = if i + 1 < section_starts.len() { section_starts[i + 1] } else { total };
            spans.push((start, end.max(start + 0.1)));
        }
        return (spans, true);
    }
    let step = total / pages as f64;
    ((0..pages).map(|i| (i as f64 * step, (i as f64 + 1.0) * step)).collect(), false)
}

/// The navigation document — EPUB 3's table of contents, and a hard requirement.
///
/// Nested by `nav_depth`, and a page at depth 0 is not listed at all. Both matter for a book rather
/// than an export: a contents page that opens with "Title page · Copyright · Dedication" is the
/// mark of a converter, and a forty-page edition with forty top-level entries has a contents page
/// nobody can use. A depth-2 page hangs under the last depth-1 entry, which is how a volume's
/// chapters sit under their part.
///
/// The `landmarks` nav is emitted alongside it. It is optional in the spec and expected in practice:
/// it is how a reading system knows where the book actually begins, and several stores check for it.
pub fn nav_xhtml(title: &str, pages: &[Page]) -> String {
    let mut items = String::new();
    let mut open_child = false;
    for p in pages.iter().filter(|p| p.nav_depth > 0) {
        let label = if p.heading.is_empty() { p.id.clone() } else { p.heading.clone() };
        let entry = format!("<a href=\"{}.xhtml\">{}</a>", xml_escape(&p.id), xml_escape(&label));
        if p.nav_depth >= 2 {
            // A nested entry with no parent yet would be invalid nesting, so it is promoted rather
            // than dropped: losing a chapter from the contents is worse than losing its indent.
            if items.is_empty() { items.push_str(&format!("      <li>{entry}</li>\n")); continue; }
            if !open_child {
                // Reopen the parent <li> to hang an <ol> inside it.
                let cut = items.rfind("</li>\n").map(|i| i).unwrap_or(items.len());
                items.truncate(cut);
                items.push_str("\n        <ol>\n");
                open_child = true;
            }
            items.push_str(&format!("          <li>{entry}</li>\n"));
        } else {
            if open_child { items.push_str("        </ol>\n      "); open_child = false; }
            items.push_str(&format!("      <li>{entry}</li>\n"));
        }
    }
    if open_child { items.push_str("        </ol>\n      </li>\n"); }

    // Where the book begins: the first page that is not front matter, or the first page there is.
    let start = pages.iter()
        .find(|p| matches!(p.role, Role::Body | Role::Part))
        .or_else(|| pages.first());
    let mut landmarks = String::new();
    if let Some(p) = pages.iter().find(|p| p.role == Role::TitlePage) {
        landmarks.push_str(&format!(
            "      <li><a epub:type=\"titlepage\" href=\"{}.xhtml\">Title page</a></li>\n",
            xml_escape(&p.id)));
    }
    if let Some(p) = start {
        landmarks.push_str(&format!(
            "      <li><a epub:type=\"bodymatter\" href=\"{}.xhtml\">Beginning</a></li>\n",
            xml_escape(&p.id)));
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
         <head><title>{title}</title></head>\n<body>\n  \
         <nav epub:type=\"toc\" id=\"toc\">\n    <h1>{title}</h1>\n    <ol>\n{items}    </ol>\n  </nav>\n  \
         <nav epub:type=\"landmarks\" id=\"landmarks\" hidden=\"hidden\">\n    \
         <ol>\n{landmarks}    </ol>\n  </nav>\n\
         </body>\n</html>\n",
        title = xml_escape(title), items = items, landmarks = landmarks,
    )
}

pub fn container_xml() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
     <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n  \
     <rootfiles>\n    \
     <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\n  \
     </rootfiles>\n</container>\n"
}

/// Page styling. Deliberately restrained: readers override most of it, and a graphic-novel page is
/// carried by the art rather than by typography tricks that break on a six-inch screen.
pub fn stylesheet() -> String {
    // The bubble rules come from typography.rs so the EPUB and the exported images cannot drift
    // apart about what a speech bubble is.
    format!("{}{}", crate::typography::BUBBLE_CSS,
    "html, body { margin: 0; padding: 0; }\n\
     body { font-family: Georgia, serif; line-height: 1.5; padding: 1em; }\n\
     h2 { font-size: 1.1em; letter-spacing: 0.08em; text-transform: uppercase; \
          font-weight: normal; opacity: 0.7; }\n\
     figure.panel { margin: 0 0 1em 0; text-align: center; page-break-inside: avoid; }\n\
     figure.panel img { max-width: 100%; height: auto; }\n\
     figcaption { font-size: 0.8em; font-style: italic; opacity: 0.75; margin-top: 0.4em; }\n\
     p { margin: 0 0 0.8em 0; text-indent: 0; }\n\
     /* Front and back matter. A title page set like a chapter is the clearest sign a book was \n\
        converted rather than made, and these rules cost nothing on a six-inch screen. */\n\
     .titlepage, .part { text-align: center; padding-top: 25%; }\n\
     .titlepage h1 { font-size: 2em; font-weight: normal; letter-spacing: 0.04em; margin: 0 0 0.3em; }\n\
     .titlepage h2, .part h2 { font-size: 1em; letter-spacing: 0.15em; text-transform: uppercase; \
          opacity: 0.6; border: 0; }\n\
     .titlepage p { font-style: italic; opacity: 0.85; }\n\
     .part h2 { font-size: 1.4em; text-transform: none; letter-spacing: 0.02em; opacity: 1; }\n\
     .copyright { font-size: 0.8em; opacity: 0.8; }\n\
     .copyright h2 { display: none; }\n\
     .frontmatter, .backmatter { text-align: center; }\n\
     .frontmatter p, .backmatter p { font-style: italic; }\n\
     .backmatter { text-align: left; }\n\
     .backmatter p { font-style: normal; }\n")
}

fn media_type(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.ends_with(".png") { "image/png" }
    else if lower.ends_with(".gif") { "image/gif" }
    else if lower.ends_with(".svg") { "image/svg+xml" }
    else if lower.ends_with(".webp") { "image/webp" }
    else if lower.ends_with(".mp3") { "audio/mpeg" }
    else if lower.ends_with(".m4a") || lower.ends_with(".mp4") { "audio/mp4" }
    else if lower.ends_with(".wav") { "audio/wav" }
    else if lower.ends_with(".ogg") || lower.ends_with(".oga") { "audio/ogg" }
    else { "image/jpeg" }
}

/// Assemble the whole book. Returns the EPUB bytes.
///
/// `images` and `audio` are `(file name, bytes)`; the names must match what the pages reference.
pub fn build(
    meta: &Metadata,
    pages: &[Page],
    images: &[(String, Vec<u8>)],
    audio: &[(String, Vec<u8>)],
    cover: Option<&str>,
    modified_iso: &str,
) -> Vec<u8> {
    let mut zip = Zip::new();
    // First, stored, no extra field — this is the rule readers check at a fixed offset.
    zip.add("mimetype", b"application/epub+zip");
    zip.add("META-INF/container.xml", container_xml().as_bytes());

    let image_names: Vec<String> = images.iter().map(|(n, _)| n.clone()).collect();
    let audio_names: Vec<String> = audio.iter().map(|(n, _)| n.clone()).collect();
    zip.add("OEBPS/content.opf", content_opf(
        meta, pages, &image_names, &audio_names, cover, modified_iso,
    ).as_bytes());
    zip.add("OEBPS/nav.xhtml", nav_xhtml(&meta.title, pages).as_bytes());
    zip.add("OEBPS/style.css", stylesheet().as_bytes());
    let audio_name = audio.first().map(|(n, _)| n.clone()).unwrap_or_default();
    for p in pages {
        zip.add(&format!("OEBPS/{}.xhtml", p.id), page_xhtml(p, &meta.title).as_bytes());
        let narrator = p.audio.as_deref()
            .filter(|n| audio.iter().any(|(have, _)| have == n))
            .unwrap_or(&audio_name);
        if p.has_overlay && !narrator.is_empty() {
            zip.add(&format!("OEBPS/{}.smil", p.id),
                    page_smil(&p.id, narrator, p.span.0, p.span.1).as_bytes());
        }
    }
    for (name, bytes) in images {
        zip.add(&format!("OEBPS/images/{name}"), bytes);
    }
    for (name, bytes) in audio {
        zip.add(&format!("OEBPS/audio/{name}"), bytes);
    }
    zip.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(title: &str) -> Metadata {
        Metadata::new(title, "Lightkid", "en", "urn:uuid:1")
    }

    fn sample_pages() -> Vec<Page> {
        vec![
            Page {
                id: "page-01".into(),
                heading: "Genesis 1".into(),
                lines: vec!["Light & dark, divided".into(), "".into()],
                image: Some("panel-01.jpg".into()),
                caption: Some("The first morning".into()),
                dialogue: Some("Let there be light".into()),
                bubble_kind: "speech".into(),
                speaker_at: (0.5, 0.7),
                has_overlay: true,
                span: (0.0, 30.0),
                audio: None,
                role: Role::Body,
                nav_depth: 1,
            },
            Page {
                id: "page-02".into(),
                heading: "Genesis 2".into(),
                lines: vec!["A garden, and a name for every living thing".into()],
                image: None,
                caption: None,
                dialogue: None,
                bubble_kind: String::new(),
                speaker_at: (0.5, 0.7),
                has_overlay: true,
                span: (30.0, 62.5),
                audio: None,
                role: Role::Body,
                nav_depth: 1,
            },
        ]
    }

    #[test]
    fn a_read_along_page_declares_its_overlay_and_ships_the_smil() {
        // The overlay is what makes this read *along* rather than merely carry a file.
        let images = vec![("panel-01.jpg".to_string(), vec![0xFF, 0xD8])];
        let audio = vec![("song.mp3".to_string(), b"ID3".to_vec())];
        let epub = build(&meta("Genesis"), &sample_pages(),
                         &images, &audio, None, "2026-07-26T00:00:00Z");
        let text = String::from_utf8_lossy(&epub);
        assert!(text.contains("page-01.smil"), "the overlay file is missing from the archive");
        assert!(text.contains("media-overlay=\"smil-page-01\""), "the page does not declare it");
        assert!(text.contains("application/smil+xml"), "the overlay is not manifested");
    }

    #[test]
    fn the_smil_points_at_a_target_the_page_actually_has() {
        // A text src pointing at an id that does not exist makes the overlay silently do nothing.
        let pages = sample_pages();
        let smil = page_smil(&pages[0].id, "song.mp3", 0.0, 30.0);
        assert!(smil.contains("page-01.xhtml#body-page-01"), "{smil}");
        let html = page_xhtml(&pages[0], "Genesis");
        assert!(html.contains("id=\"body-page-01\""), "the page must carry that id: {html}");
        assert!(smil.contains("clipBegin=\"0.000s\"") && smil.contains("clipEnd=\"30.000s\""));
    }

    #[test]
    fn spans_come_from_the_analysis_when_there_is_one_and_say_when_they_do_not() {
        // Equal division reads along roughly, which beats no read-along — but "roughly" must not be
        // advertised as "synced", so the caller is told which it got.
        let (spans, real) = page_spans(120.0, &[0.0, 40.0, 80.0], 3);
        assert!(real, "three sections for three pages is real timing");
        assert_eq!(spans[0], (0.0, 40.0));
        assert_eq!(spans[2], (80.0, 120.0));

        let (guessed, real) = page_spans(120.0, &[], 4);
        assert!(!real, "no sections means a guess");
        assert_eq!(guessed[0], (0.0, 30.0));
        assert_eq!(guessed[3].1, 120.0);

        // Never a zero-length or inverted span, whatever the input.
        let (odd, _) = page_spans(0.0, &[], 2);
        assert!(odd.iter().all(|(a, b)| b > a), "{odd:?}");
        assert_eq!(page_spans(10.0, &[], 0).0.len(), 0);
    }

    #[test]
    fn crc32_matches_the_known_value() {
        // The IEEE check value every zip implementation agrees on.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn mimetype_is_first_and_stored() {
        let epub = build(&meta("Genesis"), &sample_pages(),
                         &[], &[], None, "2026-07-25T00:00:00Z");
        // Local header at offset 0, method field (offset 8) must be 0 = stored, and the name must
        // follow immediately. Readers check exactly this, and a deflated mimetype fails to open with
        // no useful message.
        assert_eq!(&epub[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        assert_eq!(u16::from_le_bytes([epub[8], epub[9]]), 0, "mimetype must be stored");
        assert_eq!(&epub[30..38], b"mimetype");
        assert_eq!(&epub[38..(38 + 20)], b"application/epub+zip");
    }

    #[test]
    fn the_archive_ends_with_a_directory_of_every_entry() {
        let images = vec![("panel-01.jpg".to_string(), vec![0xFF, 0xD8, 0xFF])];
        let audio = vec![("song.mp3".to_string(), vec![0x49, 0x44, 0x33])];
        let epub = build(&meta("Genesis"), &sample_pages(),
                         &images, &audio, Some("panel-01.jpg"), "2026-07-25T00:00:00Z");
        // mimetype, container, opf, nav, css, 2 pages, 2 overlays, 1 image, 1 audio = 11.
        // The overlays are there because both sample pages carry one and there is audio to narrate with.
        let n = epub.windows(4).filter(|w| *w == [0x50, 0x4b, 0x01, 0x02]).count();
        assert_eq!(n, 11, "every file needs a central directory record");
        assert!(epub.windows(4).any(|w| w == [0x50, 0x4b, 0x05, 0x06]), "missing end-of-directory");
    }

    #[test]
    fn every_file_in_the_zip_is_also_in_the_manifest() {
        // A file present in the zip but missing from the manifest does not exist to a reader. This is
        // the commonest way a hand-built EPUB silently loses its art.
        let pages = sample_pages();
        let opf = content_opf(&meta("Genesis"), &pages,
                             &["panel-01.jpg".to_string()], &["song.mp3".to_string()],
                             Some("panel-01.jpg"), "2026-07-25T00:00:00Z");
        assert!(opf.contains("href=\"page-01.xhtml\""));
        assert!(opf.contains("href=\"page-02.xhtml\""));
        assert!(opf.contains("href=\"images/panel-01.jpg\""));
        assert!(opf.contains("href=\"audio/song.mp3\""), "the audio is the point of using EPUB");
        assert!(opf.contains("media-type=\"audio/mpeg\""));
        assert!(opf.contains("properties=\"cover-image\""));
        assert!(opf.contains("<item id=\"nav\""), "EPUB 3 requires a nav document");
        // Reading order covers every page, in order.
        let first = opf.find("idref=\"page-01\"").expect("page 1 in spine");
        let second = opf.find("idref=\"page-02\"").expect("page 2 in spine");
        assert!(first < second, "spine order decides reading order");
    }

    #[test]
    fn an_ampersand_in_the_text_does_not_corrupt_the_book() {
        let pages = sample_pages();
        let html = page_xhtml(&pages[0], "Genesis & Exodus");
        assert!(html.contains("Light &amp; dark"), "{html}");
        assert!(html.contains("<title>Genesis &amp; Exodus</title>"));
        assert!(!html.contains("Light & dark"), "a bare ampersand makes the file unparseable");
        // An empty line is not an empty paragraph.
        assert_eq!(html.matches("<p>").count(), 1);
    }

    /// Writes a real book to the temp dir so an independent zip implementation can check it — a
    /// hand-rolled archive writer that only its own reader accepts is worth nothing.
    #[test]
    fn writes_a_file_an_outside_reader_can_open() {
        let images = vec![("panel-01.jpg".to_string(), vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10])];
        let audio = vec![("song.mp3".to_string(), b"ID3\x03\x00\x00\x00fake".to_vec())];
        let epub = build(&meta("Genesis & Light"), &sample_pages(),
                         &images, &audio, Some("panel-01.jpg"), "2026-07-25T00:00:00Z");
        let path = std::env::temp_dir().join("bm-epub-selftest.epub");
        std::fs::write(&path, &epub).expect("write");
        eprintln!("EPUB_SELFTEST {}", path.display());
        assert!(epub.len() > 1000);
    }

    #[test]
    fn the_metadata_a_store_reads_survives_into_the_package() {
        // Every one of these is optional in the format and required in practice: a book without
        // them is accepted by a store and then sorts nowhere and appears in no category.
        let mut m = meta("Genesis");
        m.subtitle = Some("A book of beginnings".into());
        m.publisher = Some("Lightkid Press".into());
        m.description = Some("Seven days & a garden".into());
        m.rights = Some("© 2026".into());
        m.subjects = vec!["Religion".into(), "Poetry".into()];
        m.series = Some("The Pentateuch".into());
        m.series_index = Some(1);
        m.pubdate = Some("2026-07-25".into());
        m.isbn = Some("9781234567897".into());
        m.contributors = vec![("ill".into(), "A. Painter".into())];
        let opf = content_opf(&m, &sample_pages(), &[], &[], None, "2026-07-25T00:00:00Z");

        assert!(opf.contains("<dc:publisher>Lightkid Press</dc:publisher>"));
        assert!(opf.contains("<dc:rights>© 2026</dc:rights>"));
        assert!(opf.contains("<dc:date>2026-07-25</dc:date>"));
        assert!(opf.contains("<dc:subject>Religion</dc:subject>"));
        assert!(opf.contains("<dc:subject>Poetry</dc:subject>"));
        assert!(opf.contains("urn:isbn:9781234567897"));
        assert!(opf.contains("belongs-to-collection"), "a series is how a store groups a set");
        assert!(opf.contains("group-position"));
        assert!(opf.contains("<dc:contributor id=\"contrib0\">A. Painter</dc:contributor>"));
        assert!(opf.contains("scheme=\"marc:relators\">ill<"), "the role must be a relator code");
        // Description text is escaped like everything else.
        assert!(opf.contains("Seven days &amp; a garden"));
    }

    #[test]
    fn nothing_to_say_means_no_empty_element_rather_than_an_empty_one() {
        // <dc:publisher/> is read by a store as a publisher named "", which is worse than absent.
        let opf = content_opf(&meta("Genesis"), &sample_pages(), &[], &[], None, "2026-07-25T00:00:00Z");
        for tag in ["dc:publisher", "dc:rights", "dc:description", "dc:date", "dc:subject"] {
            assert!(!opf.contains(&format!("<{tag}>")), "{tag} was emitted with nothing in it");
        }
        assert!(!opf.contains("belongs-to-collection"));
    }

    #[test]
    fn a_series_number_without_a_series_sorts_nowhere_so_it_is_not_emitted() {
        let mut m = meta("Genesis");
        m.series_index = Some(3);
        let opf = content_opf(&m, &sample_pages(), &[], &[], None, "2026-07-25T00:00:00Z");
        assert!(!opf.contains("group-position"));
    }

    #[test]
    fn front_matter_is_typed_so_a_reader_knows_where_the_book_begins() {
        let pages = vec![
            Page { role: Role::TitlePage, nav_depth: 0, ..Page::body("front-title", "Genesis", vec![]) },
            Page { role: Role::Copyright, nav_depth: 0, ..Page::body("front-copyright", "Copyright", vec!["© 2026".into()]) },
            Page::body("page-01", "In the beginning", vec!["Light".into()]),
        ];
        let title = page_xhtml(&pages[0], "Genesis");
        assert!(title.contains("epub:type=\"titlepage\""));
        assert!(title.contains("<h1>Genesis</h1>"), "the book's name is the one h1: {title}");
        assert!(page_xhtml(&pages[1], "Genesis").contains("epub:type=\"copyright-page\""));
        assert!(page_xhtml(&pages[2], "Genesis").contains("<h2>In the beginning</h2>"));

        let nav = nav_xhtml("Genesis", &pages);
        // The contents proper — the landmarks nav below it legitimately names the title page.
        let toc = &nav[..nav.find("landmarks").expect("a landmarks nav")];
        assert!(!toc.contains("front-title"), "a contents page does not list its own title page");
        assert!(!toc.contains("front-copyright"));
        assert!(toc.contains("page-01.xhtml"));
        // Landmarks say where the body starts — the first non-front-matter page, not page one of
        // the file.
        assert!(nav.contains("epub:type=\"bodymatter\" href=\"page-01.xhtml\""), "{nav}");
        assert!(nav.contains("epub:type=\"titlepage\" href=\"front-title.xhtml\""));
    }

    #[test]
    fn chapters_nest_under_their_part_in_the_contents() {
        let pages = vec![
            Page { role: Role::Part, nav_depth: 1, ..Page::body("part-1", "Part One", vec![]) },
            Page { nav_depth: 2, ..Page::body("ch-1", "Genesis", vec![]) },
            Page { nav_depth: 2, ..Page::body("ch-2", "Exodus", vec![]) },
            Page { role: Role::Part, nav_depth: 1, ..Page::body("part-2", "Part Two", vec![]) },
            Page { nav_depth: 2, ..Page::body("ch-3", "Psalms", vec![]) },
        ];
        let nav = nav_xhtml("The Pentateuch", &pages);
        // Two parts at the top, and each chapter inside a nested list rather than beside them.
        assert_eq!(nav.matches("<ol>").count(), 4, "toc + two nested + landmarks: {nav}");
        assert_eq!(nav.matches("</ol>").count(), 4);
        let part2 = nav.find("part-2.xhtml").expect("part two");
        let ch2 = nav.find("ch-2.xhtml").expect("exodus");
        assert!(ch2 < part2, "a chapter of part one must close before part two opens");
    }

    #[test]
    fn a_nested_entry_with_no_parent_is_promoted_rather_than_dropped() {
        // Invalid nesting would make the nav document unparseable; losing a chapter from the
        // contents is worse than losing its indent.
        let pages = vec![
            Page { nav_depth: 2, ..Page::body("ch-1", "Genesis", vec![]) },
            Page { nav_depth: 1, ..Page::body("ch-2", "Exodus", vec![]) },
        ];
        let nav = nav_xhtml("Book", &pages);
        assert!(nav.contains("ch-1.xhtml"));
        assert!(nav.contains("ch-2.xhtml"));
        assert_eq!(nav.matches("<ol>").count(), 2, "toc + landmarks, no orphan nesting: {nav}");
    }

    #[test]
    fn a_page_without_art_still_renders_as_a_page() {
        let pages = sample_pages();
        let html = page_xhtml(&pages[1], "Genesis");
        assert!(!html.contains("<figure"), "no art, no empty figure");
        assert!(html.contains("A garden"));
    }
}
