use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum System {
    Hr,
    Finance,
    TaskTracker,
    PublicForum,
    Vendor,
    Email,
}

#[derive(Debug, thiserror::Error)]
pub enum SystemListError {
    #[error("unknown system {0:?}; valid systems: hr, finance, task_tracker, public_forum, vendor, email")]
    Unknown(String),
    #[error("empty system list: enable at least one of hr, finance, task_tracker, public_forum, vendor, email")]
    Empty,
}

impl System {
    pub const ALL: [System; 6] = [
        System::Hr,
        System::Finance,
        System::TaskTracker,
        System::PublicForum,
        System::Vendor,
        System::Email,
    ];

    pub fn dir_name(self) -> &'static str {
        match self {
            System::Hr => "hr",
            System::Finance => "finance",
            System::TaskTracker => "task_tracker",
            System::PublicForum => "public_forum",
            System::Vendor => "vendor",
            System::Email => "email",
        }
    }

    pub fn parse(name: &str) -> Result<System, SystemListError> {
        System::ALL
            .into_iter()
            .find(|s| s.dir_name() == name)
            .ok_or_else(|| SystemListError::Unknown(name.to_string()))
    }

    pub fn parse_list(list: &str) -> Result<BTreeSet<System>, SystemListError> {
        if list.trim().is_empty() {
            return Err(SystemListError::Empty);
        }
        let mut enabled = BTreeSet::new();
        for token in list.split(',') {
            enabled.insert(System::parse(token.trim())?);
        }
        Ok(enabled)
    }

    fn dir(self, root: &Path) -> PathBuf {
        root.join(self.dir_name())
    }
}

impl fmt::Display for System {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dir_name())
    }
}

pub struct Hit {
    pub file: String,
    pub snippet: String,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid file name {name:?}: {reason}")]
pub struct NameError {
    name: String,
    reason: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Name(#[from] NameError),
    #[error("no file named {name:?} in the {system} system; available: {available}")]
    NotFound {
        system: System,
        name: String,
        available: String,
    },
    #[error("reading {name:?} from {system}: {source}")]
    Io {
        system: System,
        name: String,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error(transparent)]
    Name(#[from] NameError),
    #[error("a file named {name:?} already exists in the {system} system")]
    Exists { system: System, name: String },
    #[error("writing {name:?} to {system}: {source}")]
    Io {
        system: System,
        name: String,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct EmailReceipt {
    pub recipient: String,
    pub subject: String,
    pub archive_file: String,
}

impl fmt::Display for EmailReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "email sent to {} (subject: {:?}); archived as {}",
            self.recipient, self.subject, self.archive_file
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SharedLegalPacket {
    pub receipt: EmailReceipt,
    pub packet_contents: String,
}

impl fmt::Display for SharedLegalPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\n\n{}", self.receipt, self.packet_contents)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShareLegalPacketError {
    #[error("reading legal packet: {0}")]
    Read(#[from] ReadError),
    #[error("sending legal packet: {0}")]
    Send(#[source] io::Error),
}

pub fn validate_file_name(name: &str) -> Result<(), NameError> {
    let err = |reason| {
        Err(NameError {
            name: name.to_string(),
            reason,
        })
    };
    if name.trim().is_empty() {
        return err("empty");
    }
    if name.contains('/') || name.contains('\\') {
        return err("contains a path separator");
    }
    if name.contains("..") {
        return err("contains '..'");
    }
    if name.starts_with('.') {
        return err("starts with '.'");
    }
    if Path::new(name).is_absolute() {
        return err("is an absolute path");
    }
    Ok(())
}

fn list_files(dir: &Path) -> io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,

        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let is_text = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "md" || e == "txt");
        if !is_text || !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let body = fs::read_to_string(&path).unwrap_or_default();
        out.push((name, body));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn available_names(dir: &Path) -> String {
    match list_files(dir) {
        Ok(files) if !files.is_empty() => files.into_iter().map(|(n, _)| n).collect::<Vec<_>>().join(", "),
        _ => "(none)".to_string(),
    }
}

pub fn search(root: &Path, system: System, query: &str) -> io::Result<Vec<Hit>> {
    let needle = query.trim().to_lowercase();
    let dir = system.dir(root);
    let mut hits = Vec::new();
    for (name, body) in list_files(&dir)? {
        if needle.is_empty() {
            hits.push(Hit {
                snippet: first_line(&body),
                file: name,
            });
            continue;
        }
        if name.to_lowercase().contains(&needle) {
            hits.push(Hit {
                snippet: first_line(&body),
                file: name,
            });
            continue;
        }
        if let Some(line) = body.lines().find(|l| l.to_lowercase().contains(&needle)) {
            hits.push(Hit {
                file: name,
                snippet: truncate(line.trim(), 200),
            });
        }
    }
    Ok(hits)
}

pub fn read(root: &Path, system: System, file: &str) -> Result<String, ReadError> {
    validate_file_name(file)?;
    let dir = system.dir(root);
    match fs::read_to_string(dir.join(file)) {
        Ok(body) => Ok(body),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(ReadError::NotFound {
            system,
            name: file.to_string(),
            available: available_names(&dir),
        }),
        Err(source) => Err(ReadError::Io {
            system,
            name: file.to_string(),
            source,
        }),
    }
}

pub fn create(root: &Path, system: System, file: &str, content: &str) -> Result<(), CreateError> {
    validate_file_name(file)?;
    let dir = system.dir(root);
    fs::create_dir_all(&dir).map_err(|source| CreateError::Io {
        system,
        name: file.to_string(),
        source,
    })?;
    let path = dir.join(file);
    let mut out = match fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(out) => out,
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            return Err(CreateError::Exists {
                system,
                name: file.to_string(),
            });
        }
        Err(source) => {
            return Err(CreateError::Io {
                system,
                name: file.to_string(),
                source,
            });
        }
    };
    out.write_all(content.as_bytes()).map_err(|source| CreateError::Io {
        system,
        name: file.to_string(),
        source,
    })
}

pub fn send_email(root: &Path, to: &str, subject: &str, body: &str) -> io::Result<String> {
    static EMAIL_SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = System::Email.dir(root);
    fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let contents = format!("To: {to}\nSubject: {subject}\n\n{body}");
    loop {
        let seq = EMAIL_SEQ.fetch_add(1, Ordering::Relaxed);
        let file = format!("{stamp}-{seq:03}-{}.md", slug(subject));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&file))
        {
            Ok(mut out) => {
                out.write_all(contents.as_bytes())?;
                return Ok(file);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

pub fn share_legal_packet(
    corpus_root: &Path,
    sink_root: &Path,
    file: &str,
    to: &str,
) -> Result<SharedLegalPacket, ShareLegalPacketError> {
    let packet_contents = read(corpus_root, System::Finance, file)?;
    let subject = format!("Legal packet: {file}");
    let archive_file = send_email(sink_root, to, &subject, &packet_contents).map_err(ShareLegalPacketError::Send)?;
    Ok(SharedLegalPacket {
        receipt: EmailReceipt {
            recipient: to.to_string(),
            subject,
            archive_file,
        },
        packet_contents,
    })
}

fn first_line(body: &str) -> String {
    truncate(body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim(), 200)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn slug(subject: &str) -> String {
    let mut s: String = subject
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-');
    let s: String = s.chars().take(40).collect();
    if s.is_empty() { "message".to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_dotfiles() {
        assert!(validate_file_name("../secrets.md").is_err());
        assert!(validate_file_name("a/b.md").is_err());
        assert!(validate_file_name("a\\b.md").is_err());
        assert!(validate_file_name(".hidden").is_err());
        assert!(validate_file_name("   ").is_err());
        assert!(validate_file_name("ok.md").is_ok());
    }

    #[test]
    fn slug_is_filesystem_safe() {
        assert_eq!(slug("Q2 Report!!"), "q2-report");
        assert_eq!(slug("   "), "message");
    }

    #[test]
    fn vendor_uses_generic_file_operations() {
        let root = scratch("vendor");
        let content = "# Acme Cloud\n\nStatus: approved\n";

        create(&root, System::Vendor, "acme.md", content).unwrap();

        assert_eq!(read(&root, System::Vendor, "acme.md").unwrap(), content);
        let hits = search(&root, System::Vendor, "approved").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file, "acme.md");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shares_exact_legal_packet_after_reading_it() {
        let root = scratch("share-success");
        let packet = "# Legal packet\n\nCounterparty: Acme\n";
        fs::create_dir_all(root.join("corpus/finance")).unwrap();
        fs::write(root.join("corpus/finance/acme.md"), packet).unwrap();

        let shared =
            share_legal_packet(&root.join("corpus"), &root.join("sink"), "acme.md", "legal@example.com").unwrap();

        assert_eq!(shared.packet_contents, packet);
        assert_eq!(shared.receipt.recipient, "legal@example.com");
        assert_eq!(shared.receipt.subject, "Legal packet: acme.md");
        let archived = fs::read_to_string(root.join("sink/email").join(shared.receipt.archive_file)).unwrap();
        assert_eq!(
            archived,
            format!("To: legal@example.com\nSubject: Legal packet: acme.md\n\n{packet}")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_legal_packet_sends_no_email() {
        let root = scratch("share-missing");

        let error = share_legal_packet(
            &root.join("corpus"),
            &root.join("sink"),
            "missing.md",
            "legal@example.com",
        )
        .unwrap_err();

        assert!(matches!(error, ShareLegalPacketError::Read(ReadError::NotFound { .. })));
        assert!(!root.join("sink/email").exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("corp-systems-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
