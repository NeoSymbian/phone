// SPDX-License-Identifier: GPL-3.0-only
//! Files: the contacts book in our private directory (`C:\private\e0005001\contacts.dat`, the `contacts` crate's
//! NCT1 format), written through `contacts.new` so a cut never loses it; an unreadable book is kept aside as
//! `contacts.bad`. The first start copies the book neoui's shell kept (`C:\private\e0002001\contacts.dat`, before
//! Phone became this app) and sets `contacts.migrated`. vCards are read from `E:\` and `C:\Data\`, written to
//! `C:\Data\contacts.vcf`.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use contacts::Book;
use e32::dprintln;
use e32::fs::{File, Fs, mode};
use e32::sys::err;

const DIR: &str = "C:\\private\\e0005001\\";
const PATH: &str = "C:\\private\\e0005001\\contacts.dat";
const NEW_PATH: &str = "C:\\private\\e0005001\\contacts.new";
const BAD_PATH: &str = "C:\\private\\e0005001\\contacts.bad";
/// Reply with message's quick replies, one per line.
const REPLIES_PATH: &str = "C:\\private\\e0005001\\replies.txt";
/// The most quick replies.
pub const MAX_REPLIES: usize = 6;
/// What a phone starts with.
const DEFAULT_REPLIES: [&str; 3] = [
    "Can't talk now. I'll call you later.",
    "I'm in a meeting.",
    "I'm driving. I'll call you back.",
];
/// Where neoui's shell kept the book (C8, before NEOPHONE).
const SHELL_PATH: &str = "C:\\private\\e0002001\\contacts.dat";
pub const EXPORT_DIR: &str = "C:\\Data\\";
pub const EXPORT_PATH: &str = "C:\\Data\\contacts.vcf";
/// Folders searched for `.vcf` files to import.
pub const IMPORT_DIRS: [&str; 2] = ["E:\\", "C:\\Data\\"];
/// The largest file read (a book of 1000 full contacts is well under this).
const FILE_MAX: usize = 2 << 20;

pub fn read(fs: &Fs, path: &str) -> Result<Vec<u8>, i32> {
    let f = File::open(fs, path, mode::READ | mode::SHARE_READERS_ONLY)?;
    let size = usize::try_from(f.size()?).unwrap_or(usize::MAX);
    if size > FILE_MAX {
        return Err(-40); // KErrTooBig
    }
    let mut b = vec![0u8; size];
    let n = f.read_at(0, &mut b)?;
    b.truncate(n);
    Ok(b)
}

pub fn mkdirs(fs: &Fs, dirs: &[&str]) -> Result<(), i32> {
    for d in dirs {
        match fs.mkdir(d) {
            Ok(()) | Err(err::KERR_ALREADY_EXISTS) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// The saved book. `migrate`: there is none yet and the shell's old book should be copied (first start).
pub fn load(migrate: bool) -> (Book, bool) {
    let Ok(fs) = Fs::connect() else {
        return (Book::default(), false);
    };
    let mut paths: Vec<&str> = vec![PATH, NEW_PATH];
    if migrate {
        paths.push(SHELL_PATH);
    }
    for path in paths {
        match read(&fs, path) {
            Ok(b) => match Book::decode(&b) {
                Some(book) => {
                    dprintln!(
                        "neophone: contacts loaded from {} ({})",
                        path,
                        book.contacts.len()
                    );
                    let migrated = path == SHELL_PATH;
                    if migrated && let Err(e) = save(&book) {
                        dprintln!("neophone: migrated contacts not saved ({})", e);
                    }
                    return (book, migrated);
                }
                None if path == SHELL_PATH => {
                    dprintln!("neophone: the shell's contacts are unreadable; not copied");
                }
                None => {
                    let _ = fs.delete(BAD_PATH);
                    let r = fs.rename(path, BAD_PATH);
                    dprintln!(
                        "neophone: contacts unreadable, kept as contacts.bad ({:?})",
                        r
                    );
                    return (Book::default(), false);
                }
            },
            Err(err::KERR_NOT_FOUND | err::KERR_PATH_NOT_FOUND) => {}
            Err(e) => {
                dprintln!("neophone: contacts not read from {} ({})", path, e);
                return (Book::default(), false);
            }
        }
    }
    (Book::default(), false)
}

/// Write the book (new file, then swap), so a cut leaves the old or the new book.
pub fn save(book: &Book) -> Result<(), i32> {
    let fs = Fs::connect()?;
    mkdirs(&fs, &["C:\\private\\", DIR])?;
    File::replace(&fs, NEW_PATH, mode::SHARE_EXCLUSIVE)?.write(&book.encode())?;
    match fs.delete(PATH) {
        Ok(()) | Err(err::KERR_NOT_FOUND) => {}
        Err(e) => return Err(e),
    }
    fs.rename(NEW_PATH, PATH)
}

/// Every `.vcf` file in the import folders, as (path, bytes).
pub fn vcards() -> Vec<(String, Vec<u8>)> {
    let Ok(fs) = Fs::connect() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dir in IMPORT_DIRS {
        let Ok(entries) = fs.read_dir(dir) else {
            continue;
        };
        for e in entries {
            if e.is_dir() || !e.name.to_ascii_lowercase().ends_with(".vcf") {
                continue;
            }
            let path = alloc::format!("{dir}{}", e.name);
            match read(&fs, &path) {
                Ok(b) => out.push((path, b)),
                Err(e) => dprintln!("neophone: {} not read ({})", path, e),
            }
        }
    }
    out
}

/// The quick replies (the defaults until the user changes them).
pub fn replies() -> Vec<String> {
    let saved = Fs::connect().and_then(|fs| read(&fs, REPLIES_PATH));
    match saved {
        Ok(b) => String::from_utf8_lossy(&b)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(MAX_REPLIES)
            .map(String::from)
            .collect(),
        Err(_) => DEFAULT_REPLIES.iter().map(|s| String::from(*s)).collect(),
    }
}

pub fn save_replies(replies: &[String]) -> Result<(), i32> {
    let fs = Fs::connect()?;
    mkdirs(&fs, &["C:\\private\\", DIR])?;
    File::replace(&fs, REPLIES_PATH, mode::SHARE_EXCLUSIVE)?.write(replies.join("\n").as_bytes())
}

/// Write `text` as `C:\Data\contacts.vcf`.
pub fn export(text: &str) -> Result<(), i32> {
    let fs = Fs::connect()?;
    mkdirs(&fs, &[EXPORT_DIR])?;
    File::replace(&fs, EXPORT_PATH, mode::SHARE_EXCLUSIVE)?.write(text.as_bytes())
}
