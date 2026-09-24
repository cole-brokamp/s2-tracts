mod lookup;
use lookup::TractLookup;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const BUNDLE: &str = "s2-tracts-2010-2020-r1";
const FILES: [(&str, u64, &str); 2] = [
    (
        "tracts_2010.fgb",
        612_741_776,
        "efd9d71eb758a0134fb3c1b2fda0a8d5902008d4c6f7e5531fa4c4d32f049e82",
    ),
    (
        "tracts_2020.fgb",
        684_167_640,
        "a22f95dc3b25e58c01809abfbed5b01191869d333f6d2d62e9bf712ed7903033",
    ),
];

fn main() {
    if let Err(error) = run() {
        eprintln!("s2-tracts: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let first = args.next();
    match first.as_deref() {
        Some("lookup") => lookup(args.collect()),
        Some("data") => match args.next().as_deref() {
            Some("install") if args.next().is_none() => {
                let dir = data_dir()?;
                install_data(&dir)?;
                eprintln!("Data ready at {}", dir.display());
                Ok(())
            }
            Some("path") if args.next().is_none() => {
                println!("{}", data_dir()?.display());
                Ok(())
            }
            _ => Err("usage: s2-tracts data {install|path}".into()),
        },
        Some("--version" | "-V") => {
            println!("s2-tracts {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h") => {
            print_help();
            Ok(())
        }
        None if io::stdin().is_terminal() => {
            print_help();
            Ok(())
        }
        None => lookup(Vec::new()),
        Some(_) => lookup(std::iter::once(first.unwrap()).chain(args).collect()),
    }
}

fn print_help() {
    println!(
        "s2-tracts {}\n\nUsage:\n  s2-tracts [S2_ID ...]\n  s2-tracts data install\n  s2-tracts data path\n\nIf no IDs are given, read one unsigned level-30 S2 ID per stdin line.\nOutput is JSON Lines with string IDs and null for no strict tract match.",
        env!("CARGO_PKG_VERSION")
    );
}

fn data_dir() -> Result<PathBuf, Box<dyn Error>> {
    let base = env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or("set XDG_DATA_HOME or HOME")?;
    Ok(base.join("s2-tracts").join(BUNDLE))
}

fn lookup(mut ids: Vec<String>) -> Result<(), Box<dyn Error>> {
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
    let dir = data_dir()?;
    if !dir.join(FILES[0].0).is_file() || !dir.join(FILES[1].0).is_file() {
        return Err(format!(
            "tract data missing at {}; run `s2-tracts data install`",
            dir.display()
        )
        .into());
    }
    let mut reader = TractLookup::open(&dir)?;
    let results = reader.lookup(&ids)?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    for (id, result) in ids.iter().zip(results) {
        let line = serde_json::json!({
            "s2_cell_id": id.to_string(),
            "tract_2010": result.tract_2010,
            "tract_2020": result.tract_2020,
        });
        serde_json::to_writer(&mut out, &line)?;
        writeln!(out)?;
    }
    Ok(())
}

fn release_base() -> Result<String, Box<dyn Error>> {
    if let Ok(url) = env::var("S2_TRACTS_RELEASE_BASE_URL") {
        return Ok(url.trim_end_matches('/').to_owned());
    }
    let repository = "cole-brokamp/s2-tracts";
    Ok(format!(
        "https://github.com/{repository}/releases/download/v{}",
        env!("CARGO_PKG_VERSION")
    ))
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

fn verified(path: &Path, size: u64, hash: &str) -> Result<bool, Box<dyn Error>> {
    Ok(path.is_file() && fs::metadata(path)?.len() == size && sha256(path)? == hash)
}

fn install_data(dir: &Path) -> Result<(), Box<dyn Error>> {
    if FILES
        .iter()
        .all(|(name, size, hash)| verified(&dir.join(name), *size, hash).unwrap_or(false))
    {
        return Ok(());
    }
    if dir.exists() {
        return Err(format!(
            "{} exists but does not match the expected bundle; move it aside before reinstalling",
            dir.display()
        )
        .into());
    }
    let parent = dir.parent().ok_or("data directory needs a parent")?;
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(".s2-tracts-install-{}", std::process::id()));
    fs::create_dir(&stage)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let base = release_base()?;
        for (name, size, hash) in FILES {
            let path = stage.join(name);
            eprintln!("Downloading {name} from {base}");
            let status = Command::new("curl")
                .args(["--fail", "--location", "--retry", "3", "--output"])
                .arg(&path)
                .arg(format!("{base}/{name}"))
                .status()?;
            if !status.success() {
                return Err(format!("download failed for {name}").into());
            }
            if !verified(&path, size, hash)? {
                return Err(format!("size or SHA-256 mismatch for {name}").into());
            }
        }
        fs::rename(&stage, dir)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}
