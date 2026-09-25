mod lookup;
use lookup::{TractLookup, validate_ids};
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_VINTAGE: u16 = 2020;
const FIRST_VINTAGE: u16 = 2010;
const LAST_VINTAGE: u16 = 2025;

fn main() {
    if let Err(error) = run() {
        eprintln!("s2-tracts: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    if args == ["--version"] || args == ["-V"] {
        println!("s2-tracts {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let vintage = take_vintage(&mut args)?;
    match args.first().map(String::as_str) {
        Some("lookup") => lookup(args.into_iter().skip(1).collect(), vintage),
        Some("data") => match args.get(1).map(String::as_str) {
            Some("install") if args.len() == 2 => {
                let path = data_path(vintage)?;
                install_data(&path, vintage)?;
                eprintln!("Data ready at {}", path.display());
                Ok(())
            }
            Some("path") if args.len() == 2 => {
                println!("{}", data_path(vintage)?.display());
                Ok(())
            }
            _ => Err("usage: s2-tracts data {install|path} [--vintage YEAR]".into()),
        },
        None if io::stdin().is_terminal() => {
            print_help();
            Ok(())
        }
        _ => lookup(args, vintage),
    }
}

fn take_vintage(args: &mut Vec<String>) -> Result<u16, Box<dyn Error>> {
    let mut vintage = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--vintage" {
            if vintage.is_some() || i + 1 >= args.len() {
                return Err("supply --vintage YEAR exactly once".into());
            }
            let value: u16 = args[i + 1].parse()?;
            if !(FIRST_VINTAGE..=LAST_VINTAGE).contains(&value) {
                return Err(
                    format!("vintage must be between {FIRST_VINTAGE} and {LAST_VINTAGE}").into(),
                );
            }
            vintage = Some(value);
            args.drain(i..=i + 1);
        } else {
            i += 1;
        }
    }
    Ok(vintage.unwrap_or(DEFAULT_VINTAGE))
}

fn print_help() {
    println!(
        "s2-tracts {}\n\nUsage:\n  s2-tracts [--vintage YEAR] [S2_ID ...]\n  s2-tracts data install [--vintage YEAR]\n  s2-tracts data path [--vintage YEAR]\n\nDefault vintage: {DEFAULT_VINTAGE}. Annual vintages: {FIRST_VINTAGE}-{LAST_VINTAGE}.\nA lookup downloads its vintage on first use, then works offline.\nUse `data install` to prefetch a vintage.\nIf no IDs are given, read one unsigned level-30 S2 ID per stdin line.\nOutput is JSON Lines with string IDs and null for no strict tract match.",
        env!("CARGO_PKG_VERSION")
    );
}

fn data_path(vintage: u16) -> Result<PathBuf, Box<dyn Error>> {
    let base = env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or("set XDG_DATA_HOME or HOME")?;
    Ok(base
        .join("s2-tracts")
        .join(format!("tracts-{vintage}-{}", lookup::DATA_REVISION))
        .join(format!("tracts_{vintage}.fgb")))
}

fn lookup(mut ids: Vec<String>, vintage: u16) -> Result<(), Box<dyn Error>> {
    if ids.is_empty() {
        for (line, input) in io::stdin().lock().lines().enumerate() {
            let input = input?;
            let value = input.trim();
            if value.is_empty() {
                return Err(format!("empty S2 ID on stdin line {}", line + 1).into());
            }
            ids.push(value.to_owned());
        }
    }
    let ids: Vec<u64> = ids
        .iter()
        .enumerate()
        .map(|(i, value)| {
            value
                .parse::<u64>()
                .map_err(|_| format!("invalid unsigned S2 ID at position {}: {value}", i + 1))
        })
        .collect::<Result<_, _>>()?;
    validate_ids(&ids)?;
    if ids.is_empty() {
        return Ok(());
    }
    let path = data_path(vintage)?;
    if !path.is_file() {
        install_data(&path, vintage)?;
    }
    let mut reader = TractLookup::open(path.parent().unwrap(), vintage)?;
    let results = reader.lookup(&ids)?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    for (id, tract) in ids.iter().zip(results) {
        let line = serde_json::json!({
            "s2_cell_id": id.to_string(),
            "vintage": vintage,
            "tract": tract,
        });
        serde_json::to_writer(&mut out, &line)?;
        writeln!(out)?;
    }
    Ok(())
}

fn release_base() -> String {
    if let Ok(url) = env::var("S2_TRACTS_RELEASE_BASE_URL") {
        return url.trim_end_matches('/').to_owned();
    }
    let repository = "cole-brokamp/s2-tracts";
    format!(
        "https://github.com/{repository}/releases/download/v{}",
        env!("CARGO_PKG_VERSION")
    )
}

fn sha256(path: &Path) -> Result<String, Box<dyn Error>> {
    let commands: [(&str, &[&str]); 2] = [("shasum", &["-a", "256"]), ("sha256sum", &[])];
    for (program, args) in commands {
        if let Ok(output) = Command::new(program).args(args).arg(path).output() {
            if !output.status.success() {
                return Err(format!("{program} failed for {}", path.display()).into());
            }
            let output = String::from_utf8(output.stdout)?;
            return Ok(output
                .split_whitespace()
                .next()
                .ok_or("missing SHA-256 output")?
                .to_owned());
        }
    }
    Err("SHA-256 verification requires shasum or sha256sum".into())
}

fn download(url: &str, path: &Path) -> Result<(), Box<dyn Error>> {
    let status = Command::new("curl")
        .args(["--fail", "--location", "--retry", "3", "--output"])
        .arg(path)
        .arg(url)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("download failed: {url}").into())
    }
}

struct Asset {
    raw_bytes: u64,
    raw_sha256: String,
    archive_bytes: u64,
    archive_sha256: String,
}

fn asset(vintage: u16) -> Result<Asset, Box<dyn Error>> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../scripts/assets.lock.json"))?;
    let item = manifest
        .get(vintage.to_string())
        .ok_or(format!("no release asset for vintage {vintage}"))?;
    let field = |name: &str| -> Result<String, Box<dyn Error>> {
        Ok(item
            .get(name)
            .and_then(|v| v.as_str())
            .ok_or("invalid asset manifest")?
            .to_owned())
    };
    let bytes = |name: &str| -> Result<u64, Box<dyn Error>> {
        Ok(item
            .get(name)
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0)
            .ok_or("invalid asset manifest")?)
    };
    let result = Asset {
        raw_bytes: bytes("raw_bytes")?,
        raw_sha256: field("raw_sha256")?,
        archive_bytes: bytes("archive_bytes")?,
        archive_sha256: field("archive_sha256")?,
    };
    if [&result.raw_sha256, &result.archive_sha256]
        .iter()
        .any(|hash| hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("invalid asset manifest checksum".into());
    }
    Ok(result)
}

fn install_data(path: &Path, vintage: u16) -> Result<(), Box<dyn Error>> {
    let dir = path.parent().ok_or("data file needs a parent")?;
    let name = format!("tracts_{vintage}.fgb");
    let archive_name = format!("{name}.zst");
    let expected = asset(vintage)?;
    if path.is_file() {
        if fs::metadata(path)?.len() == expected.raw_bytes && sha256(path)? == expected.raw_sha256 {
            TractLookup::open(dir, vintage)?;
            return Ok(());
        }
    }
    if dir.exists() {
        return Err(format!(
            "{} exists but does not match the expected vintage; move it aside before reinstalling",
            dir.display()
        )
        .into());
    }
    let parent = dir.parent().ok_or("data directory needs a parent")?;
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(
        ".s2-tracts-{vintage}-install-{}",
        std::process::id()
    ));
    fs::create_dir(&stage)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let staged_file = stage.join(&name);
        let base = release_base();
        let archive = stage.join(&archive_name);
        eprintln!("Downloading {archive_name} from {base}");
        download(&format!("{base}/{archive_name}"), &archive)?;
        if fs::metadata(&archive)?.len() != expected.archive_bytes
            || sha256(&archive)? != expected.archive_sha256
        {
            return Err(format!("SHA-256 mismatch for {archive_name}").into());
        }
        zstd::stream::copy_decode(fs::File::open(&archive)?, fs::File::create(&staged_file)?)?;
        if fs::metadata(&staged_file)?.len() != expected.raw_bytes
            || sha256(&staged_file)? != expected.raw_sha256
        {
            return Err(format!("SHA-256 mismatch for {name}").into());
        }
        TractLookup::open(&stage, vintage)?;
        fs::remove_file(archive)?;
        fs::rename(&stage, dir)?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_dir_all(&stage);
            if path.is_file()
                && fs::metadata(path)?.len() == expected.raw_bytes
                && sha256(path)? == expected.raw_sha256
            {
                return Ok(());
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_vintage_has_a_pinned_asset() {
        for year in FIRST_VINTAGE..=LAST_VINTAGE {
            let pinned = asset(year).unwrap();
            assert!(pinned.archive_bytes < pinned.raw_bytes);
        }
    }

    #[test]
    fn vintage_option_selects_one_year_and_defaults_to_2020() {
        assert_eq!(take_vintage(&mut vec![]).unwrap(), 2020);
        let mut args = vec![
            "lookup".into(),
            "--vintage".into(),
            "2019".into(),
            "42".into(),
        ];
        assert_eq!(take_vintage(&mut args).unwrap(), 2019);
        assert_eq!(args, ["lookup", "42"]);
        assert!(take_vintage(&mut vec!["--vintage".into(), "2009".into()]).is_err());
    }
}
