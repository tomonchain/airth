//! Text from a scan (decision 95): a PDF made of page images, or a photo of
//! a document. On macOS, the system's own text recognition (Vision) reads
//! it, on this Mac: the document never leaves it. The reader runs under
//! `sandbox-exec` with no network, no access to the owner's home (the
//! Airth home and the businesses' folders included) and no other program,
//! and is given only a copy of the document in a fresh temporary folder.
//! Elsewhere, scans stay unreadable for now.

use anyhow::{Result, bail};

/// Pages read at most: a company document, not a book.
pub const MAX_PAGES: usize = 20;

/// What is read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Pdf,
    Image,
}

/// The text of a scan, recognized on this Mac.
#[cfg(target_os = "macos")]
pub fn text(bytes: &[u8], kind: Kind) -> Result<String> {
    mac::text(bytes, kind)
}

#[cfg(not(target_os = "macos"))]
pub fn text(_: &[u8], _: Kind) -> Result<String> {
    bail!("this looks like a scan (an image of the pages), which Airth reads only on macOS for now")
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use anyhow::Context;
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
    const OSASCRIPT: &str = "/usr/bin/osascript";
    /// A scan of twenty pages reads in well under a minute.
    const TIMEOUT: Duration = Duration::from_secs(120);

    /// Vision through JavaScript for Automation: PDFKit draws each page at
    /// twice its size, Vision reads it accurately. Pages are separated by
    /// a form feed.
    const SCRIPT: &str = r#"ObjC.import('Vision'); ObjC.import('PDFKit'); ObjC.import('AppKit');
function lines(cg) {
  const handler = $.VNImageRequestHandler.alloc.initWithCGImageOptions(cg, $.NSDictionary.dictionary);
  const req = $.VNRecognizeTextRequest.alloc.init;
  req.recognitionLevel = 0; req.usesLanguageCorrection = true;
  handler.performRequestsError($([req]), null);
  const out = [];
  const res = req.results;
  for (let i = 0; i < res.count; i++) out.push(res.objectAtIndex(i).topCandidates(1).objectAtIndex(0).string.js);
  return out.join('\n');
}
function cgOf(image) { return image.CGImageForProposedRectContextHints(null, $(), $.NSDictionary.dictionary); }
function run(argv) {
  const url = $.NSURL.fileURLWithPath(argv[0]);
  if (argv[1] === 'image') {
    const image = $.NSImage.alloc.initWithContentsOfURL(url);
    if (image.isNil()) throw new Error('not an image');
    return lines(cgOf(image));
  }
  const doc = $.PDFDocument.alloc.initWithURL(url);
  if (doc.isNil()) throw new Error('not a PDF');
  const pages = [];
  for (let i = 0; i < Math.min(doc.pageCount, Number(argv[2])); i++) {
    const page = doc.pageAtIndex(i);
    const box = page.boundsForBox(0);
    pages.push(lines(cgOf(page.thumbnailOfSizeForBox($.NSMakeSize(box.size.width * 2, box.size.height * 2), 0))));
  }
  return pages.join('\n\f\n');
}"#;

    /// No network, nothing of the owner's home, no program but the reader.
    fn profile(home: &Path) -> Result<String> {
        // The sandbox compares real paths: a home reached through a link
        // (/var is /private/var) must be named by its real one.
        let home = home.canonicalize().context("finding the home folder")?;
        let home = home
            .to_str()
            .context("the home folder's path is not text")?;
        if home.contains(['"', '\\']) {
            bail!("the home folder's path has quotes: the scan is not read");
        }
        Ok(format!(
            "(version 1)(allow default)(deny network*)(deny process-exec)(allow process-exec (literal \"{OSASCRIPT}\"))(deny file-read* (subpath \"{home}\"))(deny file-write* (subpath \"{home}\"))"
        ))
    }

    /// A fresh folder of the system's temporary space, only the owner's,
    /// removed when dropped.
    pub(super) struct Scratch(pub(super) PathBuf);

    impl Scratch {
        pub(super) fn new() -> Result<Self> {
            let dir =
                std::env::temp_dir().join(format!("airth-ocr-{}", crate::ui::random_token()?));
            std::fs::create_dir(&dir).context("creating a folder for the scan")?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
            Ok(Self(dir))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub fn text(bytes: &[u8], kind: Kind) -> Result<String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("no HOME")?;
        let scratch = Scratch::new()?;
        let input = scratch.0.join("scan");
        std::fs::write(&input, bytes)?;
        let kind = match kind {
            Kind::Pdf => "pdf",
            Kind::Image => "image",
        };
        let pages = MAX_PAGES.to_string();
        sandboxed(
            &home,
            &scratch,
            SCRIPT,
            &[input.as_os_str(), kind.as_ref(), pages.as_ref()],
        )
    }

    /// Run `script` under the reader's sandbox, `home` closed to it.
    pub(super) fn sandboxed(
        home: &Path,
        scratch: &Scratch,
        script: &str,
        args: &[&std::ffi::OsStr],
    ) -> Result<String> {
        let file = scratch.0.join("read.js");
        std::fs::write(&file, script)?;
        let mut child = Command::new(SANDBOX_EXEC)
            .arg("-p")
            .arg(profile(home)?)
            .arg(OSASCRIPT)
            .args(["-l", "JavaScript"])
            .arg(&file)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("starting the system's text recognition")?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() > TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                bail!(
                    "reading the scan took over {} s: stopped",
                    TIMEOUT.as_secs()
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let mut out = String::new();
        if let Some(mut s) = child.stdout.take() {
            s.read_to_string(&mut out)?;
        }
        if !status.success() {
            let mut err = String::new();
            if let Some(mut s) = child.stderr.take() {
                s.read_to_string(&mut err)?;
            }
            bail!("the system could not read it as a scan: {}", err.trim());
        }
        Ok(out.trim().to_string())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// A page drawn as an image, as a scanner makes it: Vision reads its
    /// words, from a PNG and from a PDF of it; the reader cannot read the
    /// owner's home.
    #[test]
    fn a_scan_is_read_on_this_mac_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("airth-ocr-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("scan.png");
        let draw = r#"ObjC.import('AppKit');
function run(argv) {
  const img = $.NSImage.alloc.initWithSize($.NSMakeSize(1200, 300));
  img.lockFocus;
  $.NSColor.whiteColor.setFill; $.NSRectFill($.NSMakeRect(0, 0, 1200, 300));
  const attrs = $.NSDictionary.dictionaryWithObjectForKey($.NSFont.systemFontOfSize(64), $.NSFontAttributeName);
  $('Registered office 42 Baker Street').drawAtPointWithAttributes($.NSMakePoint(40, 120), attrs);
  img.unlockFocus;
  const rep = $.NSBitmapImageRep.imageRepWithData(img.TIFFRepresentation);
  rep.representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $({})).writeToFileAtomically(argv[0], true);
}"#;
        let drawn = std::process::Command::new("/usr/bin/osascript")
            .args(["-l", "JavaScript", "-e", draw])
            .arg(&png)
            .status()
            .unwrap();
        assert!(drawn.success());
        let pdf = dir.join("scan.pdf");
        let made = std::process::Command::new("/usr/bin/sips")
            .args(["-s", "format", "pdf"])
            .arg(&png)
            .arg("--out")
            .arg(&pdf)
            .output()
            .unwrap();
        assert!(made.status.success());

        let from_png = text(&std::fs::read(&png).unwrap(), Kind::Image).unwrap();
        assert!(from_png.contains("Baker Street"), "{from_png}");
        let pdf_bytes = std::fs::read(&pdf).unwrap();
        // No text of its own: Airth's own reader (decision 86) finds none,
        // then Vision reads the page.
        let from_pdf = crate::extract::text(&pdf_bytes, "application/pdf").unwrap();
        assert!(from_pdf.contains("Baker Street"), "{from_pdf}");
        assert!(text(b"not a pdf", Kind::Pdf).is_err());

        // The reader cannot read the home it is given, nor reach the network,
        // nor start another program.
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("secret.txt"), "the owner's secret").unwrap();
        let scratch = mac::Scratch::new().unwrap();
        let probe = format!(
            r#"ObjC.import('Foundation');
function run() {{
  const read = $.NSString.stringWithContentsOfFileEncodingError('{}', 4, null);
  let launched = 'no';
  try {{ $.NSTask.launchedTaskWithLaunchPathArguments('/bin/ls', $(['/'])); launched = 'yes'; }} catch (e) {{}}
  const web = $.NSData.dataWithContentsOfURL($.NSURL.URLWithString('https://example.com'));
  return [read.isNil() ? 'unread' : read.js, launched, web.isNil() ? 'offline' : 'online'].join(' ');
}}"#,
            home.join("secret.txt").display()
        );
        let out = mac::sandboxed(&home, &scratch, &probe, &[]).unwrap();
        assert_eq!(out, "unread no offline");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
