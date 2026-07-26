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
    /// This page has a Media Overlay: `<page-id>.smil` narrates it.
    pub has_overlay: bool,
    /// The audio range this page covers, in seconds. Only used when `has_overlay`.
    pub span: (f64, f64),
}

/// The XHTML for one page.
pub fn page_xhtml(page: &Page, title: &str) -> String {
    let mut body = String::new();
    if !page.heading.is_empty() {
        body.push_str(&format!("    <h2>{}</h2>\n", xml_escape(&page.heading)));
    }
    if let Some(img) = &page.image {
        body.push_str(&format!(
            "    <figure class=\"panel\">\n      <img src=\"images/{}\" alt=\"{}\"/>\n",
            xml_escape(img), xml_escape(page.caption.as_deref().unwrap_or(&page.heading)),
        ));
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
         <body>\n  <section epub:type=\"chapter\" id=\"body-{id}\">\n{body}  </section>\n</body>\n</html>\n",
        lang = "en", title = xml_escape(title), body = body, id = xml_escape(&page.id),
    )
}

/// The package document: metadata, every file as a manifest item, and the reading order.
///
/// `audio` is the narration or the song itself; when present it is manifested so a reader can play it
/// and, with `overlays`, follow along.
pub fn content_opf(
    title: &str,
    author: &str,
    language: &str,
    book_id: &str,
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

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n\
         \x20 <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         \x20   <dc:identifier id=\"bookid\">{id}</dc:identifier>\n\
         \x20   <dc:title>{title}</dc:title>\n\
         \x20   <dc:creator>{author}</dc:creator>\n\
         \x20   <dc:language>{language}</dc:language>\n\
         \x20   <meta property=\"dcterms:modified\">{modified}</meta>\n{cover_meta}\
         \x20 </metadata>\n\
         \x20 <manifest>\n{manifest}  </manifest>\n\
         \x20 <spine>\n{spine}  </spine>\n\
         </package>\n",
        id = xml_escape(book_id), title = xml_escape(title), author = xml_escape(author),
        language = xml_escape(language), modified = modified_iso,
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
pub fn nav_xhtml(title: &str, pages: &[Page]) -> String {
    let mut items = String::new();
    for p in pages {
        let label = if p.heading.is_empty() { p.id.clone() } else { p.heading.clone() };
        items.push_str(&format!(
            "      <li><a href=\"{}.xhtml\">{}</a></li>\n", xml_escape(&p.id), xml_escape(&label),
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
         <head><title>{title}</title></head>\n<body>\n  \
         <nav epub:type=\"toc\" id=\"toc\">\n    <h1>{title}</h1>\n    <ol>\n{items}    </ol>\n  </nav>\n\
         </body>\n</html>\n",
        title = xml_escape(title), items = items,
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
pub fn stylesheet() -> &'static str {
    "html, body { margin: 0; padding: 0; }\n\
     body { font-family: Georgia, serif; line-height: 1.5; padding: 1em; }\n\
     h2 { font-size: 1.1em; letter-spacing: 0.08em; text-transform: uppercase; \
          font-weight: normal; opacity: 0.7; }\n\
     figure.panel { margin: 0 0 1em 0; text-align: center; page-break-inside: avoid; }\n\
     figure.panel img { max-width: 100%; height: auto; }\n\
     figcaption { font-size: 0.8em; font-style: italic; opacity: 0.75; margin-top: 0.4em; }\n\
     p { margin: 0 0 0.8em 0; text-indent: 0; }\n"
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
#[allow(clippy::too_many_arguments)]
pub fn build(
    title: &str,
    author: &str,
    language: &str,
    book_id: &str,
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
        title, author, language, book_id, pages, &image_names, &audio_names, cover, modified_iso,
    ).as_bytes());
    zip.add("OEBPS/nav.xhtml", nav_xhtml(title, pages).as_bytes());
    zip.add("OEBPS/style.css", stylesheet().as_bytes());
    let audio_name = audio.first().map(|(n, _)| n.clone()).unwrap_or_default();
    for p in pages {
        zip.add(&format!("OEBPS/{}.xhtml", p.id), page_xhtml(p, title).as_bytes());
        if p.has_overlay && !audio_name.is_empty() {
            zip.add(&format!("OEBPS/{}.smil", p.id),
                    page_smil(&p.id, &audio_name, p.span.0, p.span.1).as_bytes());
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

    fn sample_pages() -> Vec<Page> {
        vec![
            Page {
                id: "page-01".into(),
                heading: "Genesis 1".into(),
                lines: vec!["Light & dark, divided".into(), "".into()],
                image: Some("panel-01.jpg".into()),
                caption: Some("The first morning".into()),
                has_overlay: true,
                span: (0.0, 30.0),
            },
            Page {
                id: "page-02".into(),
                heading: "Genesis 2".into(),
                lines: vec!["A garden, and a name for every living thing".into()],
                image: None,
                caption: None,
                has_overlay: true,
                span: (30.0, 62.5),
            },
        ]
    }

    #[test]
    fn a_read_along_page_declares_its_overlay_and_ships_the_smil() {
        // The overlay is what makes this read *along* rather than merely carry a file.
        let images = vec![("panel-01.jpg".to_string(), vec![0xFF, 0xD8])];
        let audio = vec![("song.mp3".to_string(), b"ID3".to_vec())];
        let epub = build("Genesis", "Lightkid", "en", "urn:uuid:1", &sample_pages(),
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
        let epub = build("Genesis", "Lightkid", "en", "urn:uuid:1", &sample_pages(),
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
        let epub = build("Genesis", "Lightkid", "en", "urn:uuid:1", &sample_pages(),
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
        let opf = content_opf("Genesis", "Lightkid", "en", "urn:uuid:1", &pages,
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
        let epub = build("Genesis & Light", "Lightkid", "en", "urn:uuid:test", &sample_pages(),
                         &images, &audio, Some("panel-01.jpg"), "2026-07-25T00:00:00Z");
        let path = std::env::temp_dir().join("bm-epub-selftest.epub");
        std::fs::write(&path, &epub).expect("write");
        eprintln!("EPUB_SELFTEST {}", path.display());
        assert!(epub.len() > 1000);
    }

    #[test]
    fn a_page_without_art_still_renders_as_a_page() {
        let pages = sample_pages();
        let html = page_xhtml(&pages[1], "Genesis");
        assert!(!html.contains("<figure"), "no art, no empty figure");
        assert!(html.contains("A garden"));
    }
}
