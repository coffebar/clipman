use crate::{selector::select, serve, wipe, Multi};
use failure::Error;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::io::{stdout, Read};
use std::ops::Deref;
use std::path::PathBuf;
use std::{thread::sleep, time::Duration};
use wl_clipboard_rs::paste::{self, get_contents, get_mime_types, ClipboardType, Seat};

pub fn watch(
    idx: Vec<HashSet<Multi>>,
    hist_dir: &PathBuf,
    hist_idx: &PathBuf,
    max: usize,
) -> Result<(), Error> {
    // todo: spare space by not storing duplicate content

    // restore last session
    if !idx.is_empty() {
        serve(&idx[idx.len() - 1])?;
    };

    let text_mimes = vec![
        "text/plain;charset=utf-8",
        "text/plain",
        "STRING",
        "UTF8_STRING",
        "TEXT",
    ];
    let mut idx = idx;
    loop {
        sleep(Duration::from_millis(200));

        let mime_types = get_mime_types(ClipboardType::Regular, Seat::Unspecified)?;
        let mut sources = HashSet::new();
        let mut got_text = false;

        // apparently, requesting all mimetypes crashes wayland, I think it happens if we copy something else before we have finished
        for mime in &mime_types {
            if mime == &"COMPOUND_TEXT".to_string() || mime == &"SAVE_TARGETS".to_string() {
                // these block forever, and don't seem to be useful
                continue;
            }

            // we reduce the strain by only asking one of the typical plain text formats, which we expect to be all of same content
            let (mime, paste_mime) = if text_mimes.contains(&mime.as_str()) {
                if got_text {
                    continue;
                }
                got_text = true;
                ("text/plain;charset=utf-8".to_string(), paste::MimeType::Text)
            } else {
                (mime.to_string(), paste::MimeType::Specific(&mime))
            };

            let result = get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                paste_mime,
            );
            match result {
                Ok((mut pipe, _)) => {
                    let mut contents = vec![];
                    pipe.read_to_end(&mut contents)?;
                    sources.insert(Multi {
                        mime: mime.to_string(),
                        source: contents,
                    });
                }
                Err(paste::Error::NoSeats)
                | Err(paste::Error::ClipboardEmpty)
                | Err(paste::Error::NoMimeType) => continue,
                Err(err) => return Err(err)?,
            };
        }

        if !idx.is_empty() {
            // todo: replace this check with a variable that says "this copy is the one that follows our serving, ignore it"; this can only be done when we are event-driven, otherwise there's no warranty
            if sources == idx[idx.len() - 1] {
                continue;
            };
        };

        serve(&sources)?;

        let iter = if max != 0 && idx.len() >= max {
            idx.iter().skip(idx.len() - max + 1)
        } else {
            idx.iter().skip(0)
        };

        idx = iter.filter(|x| x != &&sources).cloned().collect();
        idx.push(sources);
        let file = File::create(hist_idx)?;
        let f = BufWriter::new(file);
        serde_cbor::to_writer(f, &idx)?;
    }
}

pub fn clear(
    idx: Vec<HashSet<Multi>>,
    hist_dir: &PathBuf,
    hist_idx: &PathBuf,
    max: usize,
    tool: String,
    toolargs: Option<String>,
    all: bool,
) -> Result<(), Error> {
    if all || idx.len() == 1 {
        wipe(hist_dir)?;
        return Ok(());
    };

    let sel = match select(&idx, max, "clear", tool, toolargs)? {
        Some(s) => s,
        None => return Ok(()),
    };

    // if we remove the latest item, we want to remove it from the WM too
    if sel == idx[idx.len() - 1] {
        serve(&idx[idx.len() - 2])?
    }

    // we write the filtered history
    let idx: Vec<HashSet<Multi>> = idx.iter().filter(|x| x.deref() != &sel).cloned().collect();
    let file = File::create(hist_idx)?;
    let f = BufWriter::new(file);
    serde_cbor::to_writer(f, &idx)?;

    Ok(())
}

pub fn pick(
    idx: Vec<HashSet<Multi>>,
    hist_idx: &PathBuf,
    max: usize,
    tool: String,
    toolargs: Option<String>,
) -> Result<(), Error> {
    let sel = match select(&idx, max, "pick", tool, toolargs)? {
        Some(s) => s,
        None => return Ok(()),
    };

    serve(&sel)?;

    // we push the pick item to the end of the queue
    let mut idx: Vec<HashSet<Multi>> = idx.iter().filter(|x| x.deref() != &sel).cloned().collect();
    idx.push(sel);

    let file = File::create(hist_idx)?;
    let f = BufWriter::new(file);
    serde_cbor::to_writer(f, &idx)?;

    Ok(())
}

pub fn export(idx: Vec<HashSet<Multi>>, format: String) -> Result<(), Error> {
    if format != "json".to_string() {
        return Err(format_err!("Unsupported output format."));
    };
    let f = BufWriter::new(stdout());
    serde_json::to_writer(f, &idx)?;

    Ok(())
}