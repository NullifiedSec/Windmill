mod art;
mod download;
mod lyric_offset;
mod lyric_source;
mod lyrics;
mod mpris;
mod player;
mod playlist;
mod seek;
mod store;
mod ui;
mod visualizer;
mod youtube;
mod ytmusic;
use std::{env, error::Error, fs, path::PathBuf, process::Command};
use store::Store;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn data_dir() -> Result<PathBuf> {
    if let Ok(dir) = env::var("WINDMILL_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Ok(base.join("windmill"))
}
fn usage() {
    println!(
        "Windmill 0.5.0 — local-first media center\n\
    ui | tui                  # launch terminal media center (default)\n\
    add <path-or-url> <title> <creator>\n\
    list | tag <id> <tag> | untag <id> <tag>\n\
    collection create <name> | collection add <name> <id> | collection show <name>\n\
    recommend <tag1,tag2,...> | play <id> | download <id> <audio|video>\n\
    lyrics <file.lrc> <elapsed-ms>
\
    playlist preview <youtube-playlist-url> | playlist import <url> [collection-name]\n\
    playlist download <url> <audio|video> | rate <id> <like|dislike|neutral>\n\
    queue list | queue add <media-id>\n\
    queue clear | queue collection <collection-name>\n\
    music search <songs|albums|artists|playlists> <query>\n\
    music browse <albums|artists> <browse-id> | mpd status | help"
    );
}
fn command(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(format!("{program} exited with {status}").into());
    }
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "help" || s == "--help") {
        usage();
        return Ok(());
    }
    let dir = data_dir()?;
    fs::create_dir_all(&dir)?;
    let store = Store::open(&dir.join("library.db"))?;
    if args.is_empty() || matches!(args[0].as_str(), "ui" | "tui") {
        return ui::run(store);
    }
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["music", "search", kind, query] => {
            for result in ytmusic::search(kind, query).map_err(|e| e.to_string())? {
                println!(
                    "{} | {} — {} | {}",
                    result.kind, result.title, result.creator, result.url
                );
            }
        }
        ["music", "browse", kind, id] => {
            let (name, tracks) = ytmusic::browse(kind, id).map_err(|e| e.to_string())?;
            println!("{} ({} tracks)", name, tracks.len());
            for track in tracks {
                println!("{} — {} | {}", track.title, track.creator, track.url);
            }
        }
        ["rate", id, value] => {
            let id = id.parse::<i64>()?;
            let preference = match *value {
                "like" => 1,
                "dislike" => -1,
                "neutral" => 0,
                _ => return Err("Rate with like/dislike/neutral".into()),
            };
            store.get(id)?;
            store.rate(id, preference)?;
            println!("Preference saved for #{id}: {value}");
        }
        ["queue", "list"] => {
            for (pos, (row, media)) in store.queue_entries()?.iter().enumerate() {
                println!(
                    "{:>3}. [{row}] #{} | {} — {}",
                    pos + 1,
                    media.id,
                    media.title,
                    media.creator
                );
            }
        }
        ["queue", "add", id] => {
            let id = id.parse::<i64>()?;
            store.get(id)?;
            let entry = store.queue_add(id)?;
            println!("Queued media #{id} as entry {entry}");
        }
        ["queue", "clear"] => {
            store.queue_clear()?;
            println!("Queue cleared");
        }
        ["queue", "collection", name] => {
            let count = store.queue_collection(name)?;
            println!("Queued {count} tracks from {name}");
        }
        ["playlist", "preview", url] => {
            let pl = playlist::fetch(url).map_err(|e| e.to_string())?;
            println!("{} | {} tracks | {}", pl.title, pl.entries.len(), pl.url);
            for (i, entry) in pl.entries.iter().enumerate() {
                println!(
                    "{:>3}. {} — {} | {}",
                    i + 1,
                    entry.title,
                    entry.creator,
                    entry.url
                );
            }
        }
        ["playlist", "import", url] | ["playlist", "import", url, _] => {
            let pl = playlist::fetch(url).map_err(|e| e.to_string())?;
            let name = args.get(3).map_or(pl.title.as_str(), String::as_str);
            let added = store.import_playlist(name, &pl.entries)?;
            println!(
                "Imported {} tracks into {name} ({added} new media)",
                pl.entries.len()
            );
        }
        ["playlist", "download", url, kind @ ("audio" | "video")] => {
            let pl = playlist::fetch(url).map_err(|e| e.to_string())?;
            store.import_playlist(&pl.title, &pl.entries)?;
            let kind = if *kind == "audio" {
                download::Kind::Audio
            } else {
                download::Kind::Video
            };
            let mut failed = 0;
            let all = store.all()?;
            for (i, entry) in pl.entries.iter().enumerate() {
                let Some(media) = all.iter().find(|m| m.source == entry.url) else {
                    failed += 1;
                    continue;
                };
                if store
                    .asset(media.id, kind.as_str())?
                    .is_some_and(|path| std::path::Path::new(&path).is_file())
                {
                    println!(
                        "{}/{}: already downloaded: {}",
                        i + 1,
                        pl.entries.len(),
                        entry.title
                    );
                    continue;
                }
                match download::fetch(media.id, &entry.url, kind, &dir) {
                    Ok(file) => {
                        store.save_asset(media.id, kind.as_str(), &file)?;
                        println!("{}/{}: saved {}", i + 1, pl.entries.len(), file.display());
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("{}/{}: {}: {e}", i + 1, pl.entries.len(), entry.title);
                    }
                }
            }
            if failed > 0 {
                return Err(format!("{failed} download(s) failed").into());
            }
        }
        ["add", path, title, creator] => {
            let id = store.add(path, title, creator)?;
            println!("Added #{id}: {title}");
        }
        ["list"] => {
            for m in store.all()? {
                println!("#{} | {} — {} | {}", m.id, m.title, m.creator, m.source);
            }
        }
        ["tag", id, tag] => store.tag(id.parse()?, tag)?,
        ["untag", id, tag] => store.untag(id.parse()?, tag)?,
        ["collection", "create", name] => store.create_collection(name)?,
        ["collection", "add", name, id] => store.add_to_collection(name, id.parse()?)?,
        ["collection", "show", name] => {
            for m in store.collection_items(name)? {
                println!("#{} | {} — {}", m.id, m.title, m.creator);
            }
        }
        ["recommend", tags] => {
            for (score, media) in
                store.recommend(tags.split(',').map(str::trim).filter(|s| !s.is_empty()))?
            {
                println!(
                    "{score:>2} | #{} | {} — {}",
                    media.id, media.title, media.creator
                );
            }
        }
        ["play", id] => {
            let media = store.get(id.parse()?)?;
            let offline = store
                .asset(media.id, "video")?
                .filter(|p| std::path::Path::new(p).is_file())
                .or(store
                    .asset(media.id, "audio")?
                    .filter(|p| std::path::Path::new(p).is_file()));
            let source = offline.as_deref().unwrap_or(&media.source);
            command("mpv", &["--", source])?;
        }
        ["download", id, kind @ ("audio" | "video")] => {
            let media = store.get(id.parse()?)?;
            let kind = if *kind == "audio" {
                download::Kind::Audio
            } else {
                download::Kind::Video
            };
            let file = download::fetch(media.id, &media.source, kind, &dir)
                .map_err(|e| format!("download: {e}"))?;
            store.save_asset(media.id, kind.as_str(), &file)?;
            println!(
                "Saved local {} asset for #{}: {}",
                kind.as_str(),
                media.id,
                file.display()
            );
        }
        ["lyrics", path, elapsed] => {
            let text = fs::read_to_string(path)?;
            let parsed = lyrics::Lyrics::parse(&text);
            if parsed.is_empty() {
                println!("No synchronized lyrics found");
            }
            let (active, window) = parsed.window(elapsed.parse::<i64>()?, 2, 2);
            for (i, line) in window.iter().enumerate() {
                println!(
                    "{} {:>8.2} | {}",
                    if active == Some(i) { ">" } else { " " },
                    line.at_ms as f64 / 1000.0,
                    line.text
                );
            }
        }
        ["mpd", "status"] => println!("{}", player::mpd_request(&["status"])?),
        ["mpd", "play", uri] => {
            let quoted = player::mpd_quote(uri);
            println!(
                "{}",
                player::mpd_request(&[&format!("add {quoted}"), "play"])?
            );
        }
        _ => {
            usage();
            return Err("unknown command or incorrect arguments".into());
        }
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("windmill: {e}");
        std::process::exit(1);
    }
}
