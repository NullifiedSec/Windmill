mod lyrics;
mod store;
use std::{
    env, error::Error, fs, io::Write, net::TcpStream, path::PathBuf, process::Command,
    time::Duration,
};
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
        "Windmill 0.1.0 — local-first media center\n\
    add <path-or-url> <title> <creator>\n\
    list | tag <id> <tag> | untag <id> <tag>\n\
    collection create <name> | collection add <name> <id> | collection show <name>\n\
    recommend <tag1,tag2,...> | play <id> | download <id> <audio|video>\n\
    lyrics <file.lrc> <elapsed-ms>
\
    mpd status | mpd play <mpd-library-uri> | help"
    );
}
fn command(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(format!("{program} exited with {status}").into());
    }
    Ok(())
}
fn mpd_request(lines: &[&str]) -> Result<String> {
    use std::io::{BufRead, BufReader};
    let host = env::var("WINDMILL_MPD_ADDR").unwrap_or_else(|_| "127.0.0.1:6600".into());
    let mut stream = TcpStream::connect(&host)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut greeting = String::new();
    reader.read_line(&mut greeting)?;
    if !greeting.starts_with("OK MPD ") {
        return Err("invalid MPD greeting".into());
    }
    let mut output = String::new();
    for line in lines {
        if line.contains(['\n', '\r']) {
            return Err("invalid MPD command".into());
        }
        stream.write_all(line.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        loop {
            let mut response = String::new();
            if reader.read_line(&mut response)? == 0 {
                return Err("MPD closed connection".into());
            }
            if response.starts_with("ACK ") {
                return Err(format!("MPD: {response}").into());
            }
            if response == "OK\n" {
                break;
            }
            output.push_str(&response);
        }
    }
    Ok(output)
}
fn mpd_quote(s: &str) -> String {
    let q = char::from_u32(34).unwrap();
    let slash = char::from_u32(92).unwrap();
    let escaped = s
        .replace(slash, &format!("{slash}{slash}"))
        .replace(q, &format!("{slash}{q}"));
    format!("{q}{escaped}{q}")
}
fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args[0] == "help" || args[0] == "--help" {
        usage();
        return Ok(());
    }
    let dir = data_dir()?;
    fs::create_dir_all(&dir)?;
    let store = Store::open(&dir.join("library.db"))?;
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
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
            command("mpv", &[&media.source])?;
        }
        ["download", id, kind @ ("audio" | "video")] => {
            let media = store.get(id.parse()?)?;
            if !media.source.starts_with("https://") && !media.source.starts_with("http://") {
                return Err("download requires an HTTP(S) media source".into());
            }
            let downloads = dir.join("downloads");
            fs::create_dir_all(&downloads)?;
            let template = downloads.join(format!("{}-%(title).150B-%(id)s.%(ext)s", media.id));
            let output = template.to_str().ok_or("non-UTF8 download path")?;
            if *kind == "audio" {
                command(
                    "yt-dlp",
                    &[
                        "--no-playlist",
                        "-x",
                        "--audio-format",
                        "best",
                        "-o",
                        output,
                        &media.source,
                    ],
                )?;
            } else {
                command("yt-dlp", &["--no-playlist", "-o", output, &media.source])?;
            }
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
        ["mpd", "status"] => println!("{}", mpd_request(&["status"])?),
        ["mpd", "play", uri] => {
            let quoted = mpd_quote(uri);
            println!("{}", mpd_request(&[&format!("add {quoted}"), "play"])?);
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

#[cfg(test)]
mod tests {
    use super::mpd_quote;
    #[test]
    fn mpd_argument_escaping() {
        let q = char::from_u32(34).unwrap();
        let slash = char::from_u32(92).unwrap();
        let input = format!("a{q}b");
        assert_eq!(mpd_quote(&input), format!("{q}a{slash}{q}b{q}"));
    }
}
