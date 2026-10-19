mod app;
mod commands;
mod config;
mod doc;
mod ipc;
mod keymap;
mod refs;
mod render;
mod script;
mod session;
mod store;
mod text;
mod ui;
mod view;

use std::path::PathBuf;

use anyhow::{bail, Result};

const USAGE: &str = "\
folio — a keyboard-first PDF reader for technical documents

usage:
  folio [FILE.pdf]                open a document (or the recent list)
  folio --print-config            print the default config.toml
  folio --print-init              print an example init.lua
  folio --init-config             write both into ~/.config/folio/ (won't overwrite)
  folio --remote CMD...           run commands in the running instance, e.g. --remote 'goto_page 12'
  folio --debug CMD FILE [PAGE]   inspect the backend: render|outline|text|links|refs|search
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some("--print-config") => {
            print!("{}", config::DEFAULT_CONFIG);
            Ok(())
        }
        Some("--print-init") => {
            print!("{}", config::DEFAULT_INIT_LUA);
            Ok(())
        }
        Some("--init-config") => init_config(),
        Some("--debug") => debug(&args[1..]),
        Some("--remote") => ipc::send(&args[1..]),
        Some(flag) if flag.starts_with("--") => bail!("unknown option {flag}\n\n{USAGE}"),
        path => run(path.map(PathBuf::from)),
    }
}

fn run(path: Option<PathBuf>) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("folio")
            .with_app_id("folio")
            .with_inner_size([1100.0, 900.0])
            .with_min_inner_size([360.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native("folio", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, path)))))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn init_config() -> Result<()> {
    let dir = config::config_dir();
    std::fs::create_dir_all(&dir)?;
    for (name, content) in [("config.toml", config::DEFAULT_CONFIG), ("init.lua", config::DEFAULT_INIT_LUA)] {
        let path = dir.join(name);
        if path.exists() {
            println!("exists, skipped: {}", path.display());
        } else {
            std::fs::write(&path, content)?;
            println!("wrote {}", path.display());
        }
    }
    Ok(())
}

/// Headless introspection of the document backend.
fn debug(args: &[String]) -> Result<()> {
    let (Some(cmd), Some(path)) = (args.first(), args.get(1)) else { bail!("usage: folio --debug CMD FILE [PAGE|QUERY]") };
    let d = doc::open(&PathBuf::from(path))?;
    let page: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    match cmd.as_str() {
        "render" => {
            let (w, h) = d.page_size(page);
            let scale = 2.0;
            let (pw, ph) = ((w * scale) as u32, (h * scale) as u32);
            let t = std::time::Instant::now();
            let mut bmp = d.render(page, scale, 0, 0, pw, ph)?;
            eprintln!("rendered {}x{} in {:?}", bmp.width, bmp.height, t.elapsed());
            if let Some(mode) = args.get(4) {
                let theme = config::Theme::preset("midnight");
                let mode = match mode.as_str() {
                    "dark" => config::PageMode::Dark,
                    "custom" => config::PageMode::Custom,
                    _ => config::PageMode::Normal,
                };
                render::recolor(&mut bmp.pixels, mode, render::Colors { fg: theme.page_fg, bg: theme.page_bg });
            }
            let out = args.get(3).cloned().unwrap_or_else(|| "page.ppm".into());
            let mut buf = format!("P6\n{} {}\n255\n", bmp.width, bmp.height).into_bytes();
            for px in bmp.pixels.chunks_exact(4) {
                buf.extend_from_slice(&px[..3]);
            }
            std::fs::write(out, buf)?;
        }
        "outline" => {
            for item in d.outline() {
                println!("{}{} -> {:?}", "  ".repeat(item.depth), item.title, item.target);
            }
        }
        "text" => {
            let t = d.page_text(page);
            println!("{} chars, {} rects", t.chars.len(), t.rects.len());
            for (s, e) in t.lines() {
                println!("{:?} {}", t.span_rects(s, e).first(), t.span_text(s, e));
            }
        }
        "links" => {
            for l in d.links(page) {
                println!("{:?} -> {:?}", l.rect, l.target);
            }
        }
        "refs" => {
            let t = d.page_text(page);
            for r in refs::detect(&t) {
                println!("{:?} {:?} '{}'", r.kind, r.label, t.span_text(r.start, r.end));
            }
        }
        "findref" => {
            // folio --debug findref FILE PAGE  — resolve every reference detected on PAGE.
            let worker = text::TextWorker::new(PathBuf::from(path), || {});
            let t = d.page_text(page);
            for (i, r) in refs::detect(&t).into_iter().enumerate() {
                worker.send(text::Request::FindRef {
                    id: i as u64,
                    kind: r.kind,
                    label: r.label.clone(),
                    from_page: page,
                    from_char: r.start,
                });
                if let Ok(text::Response::Ref { target, .. }) = worker.rx.recv() {
                    let found = target.map(|(p, rect)| {
                        let tt = d.page_text(p);
                        let line = tt
                            .lines()
                            .into_iter()
                            .find(|&(s, e)| tt.span_rects(s, e).iter().any(|lr| (lr.y0 - rect.y0).abs() < 2.0))
                            .map(|(s, e)| tt.span_text(s, e))
                            .unwrap_or_default();
                        format!("p{} «{}»", p + 1, line.chars().take(70).collect::<String>())
                    });
                    println!("{:?} {:<8} -> {}", r.kind, r.label, found.unwrap_or_else(|| "NOT FOUND".into()));
                }
            }
        }
        "search" => {
            let q = args.get(2).cloned().unwrap_or_default();
            let ignore_case = !q.chars().any(char::is_uppercase);
            let mut needle = Vec::new();
            for ch in q.chars() {
                text::expand_char(ch, &mut needle);
            }
            if ignore_case {
                needle = needle.into_iter().flat_map(char::to_lowercase).collect();
            }
            let mut total = 0;
            for p in 0..d.page_count() {
                let hits = text::find_in_page(&d.page_text(p), p, &needle, ignore_case);
                total += hits.len();
                for h in hits.iter().take(3) {
                    println!("p{} {:?}", p + 1, h.rects.first());
                }
            }
            println!("{total} hits");
        }
        _ => bail!("unknown debug command"),
    }
    Ok(())
}
