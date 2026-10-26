#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod commands;
mod config;
mod dialogs;
mod doc;
mod highlights;
mod ipc;
mod keymap;
mod open_events;
mod reference;
mod refs;
mod render;
mod script;
mod session;
mod store;
mod tabs;
#[cfg(test)]
mod tests;
mod text;
mod ui;
mod view;

use std::path::PathBuf;

use anyhow::{Result, bail};

const USAGE: &str = "\
folio — a keyboard-first PDF reader for technical documents

usage:
  folio [FILE.pdf...]             open documents, each in a tab (or the recent list); a running folio
                                  opens them as new tabs, unless --new starts a separate window
  folio --print-config            print the default config.toml
  folio --print-init              print an example init.lua
  folio --init-config             write both into ~/.config/folio/ (won't overwrite)
  folio --list-commands           print every command and its default keys (Markdown)
  folio --list-config             print every [general] setting and its default (Markdown)
  folio --list-lua                print the Lua API reference (Markdown)
  folio --update-docs [DIR]       rewrite the generated tables in the docs (default: ./docs)
  folio --version
  folio --remote CMD...           run commands in the running instance, e.g. --remote 'goto_page 12'
  folio --debug CMD FILE [PAGE]   inspect the backend: render|outline|text|links|refs|search
";

fn main() -> Result<()> {
    // Built as a GUI app on Windows (no console window when started from Explorer); reattach to the terminal's
    // console when there is one so `--help`, `--remote` and `--debug` still print.
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--print-config") => {
            print!("{}", config::DEFAULT_CONFIG);
            Ok(())
        }
        Some("--print-init") => {
            print!("{}", config::DEFAULT_INIT_LUA);
            Ok(())
        }
        Some("--init-config") => init_config(),
        Some("--list-commands") => {
            print!("{}", reference::commands_md());
            Ok(())
        }
        Some("--list-config") => {
            print!("{}", reference::config_md());
            Ok(())
        }
        Some("--list-lua") => {
            for (file, name, table) in reference::blocks().into_iter().filter(|b| b.0 == "SCRIPTING.md") {
                println!("<!-- {file}: {name} -->\n{table}");
            }
            Ok(())
        }
        Some("--update-docs") => {
            let dir = args.get(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("docs"));
            for f in reference::update_docs(&dir)? {
                println!("updated {f}");
            }
            Ok(())
        }
        Some("--debug") => debug(&args[1..]),
        Some("--remote") => ipc::send(&args[1..]),
        _ => {
            // Files, plus flags in any position (`folio paper.pdf --version`).
            let mut files = Vec::new();
            let mut new_instance = false;
            for a in &args {
                match a.as_str() {
                    "--new" => new_instance = true,
                    // Older macOS passes a process serial number when launched from Finder.
                    psn if psn.starts_with("-psn_") => {}
                    "-h" | "--help" => {
                        print!("{USAGE}");
                        return Ok(());
                    }
                    "-V" | "--version" => {
                        println!("{} {}", config::APP_NAME, env!("CARGO_PKG_VERSION"));
                        return Ok(());
                    }
                    flag if flag.starts_with('-') && flag.len() > 1 => bail!("unknown option {flag}\n\n{USAGE}"),
                    file => files.push(PathBuf::from(file)),
                }
            }
            // A running folio takes the files as new tabs, unless `--new` asks for a separate window.
            if !files.is_empty() && !new_instance && hand_off(&files) {
                return Ok(());
            }
            run(files)
        }
    }
}

fn run(files: Vec<PathBuf>) -> Result<()> {
    open_events::install();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(config::APP_NAME)
            .with_app_id(config::APP_NAME)
            .with_inner_size([1100.0, 900.0])
            .with_min_inner_size([360.0, 300.0])
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../packaging/icons/app-256.png")).unwrap_or_default()),
        // Window size and position are restored from eframe's own state file, next to folio's data.
        persist_window: true,
        persistence_path: Some(config::data_dir().join("window.ron")),
        ..Default::default()
    };
    eframe::run_native(config::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, files)))))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Open the files in the running instance, each in a new tab, and bring its window forward. False if there's no
/// running instance (or it refused), so this process should open them itself.
fn hand_off(files: &[PathBuf]) -> bool {
    let mut commands = Vec::new();
    for f in files {
        match doc::canonicalize(f) {
            Ok(p) => commands.push(format!("new_tab {}", p.display())),
            Err(_) => return false, // let the GUI report the missing file
        }
    }
    commands.push("focus_window".into());
    ipc::deliver(&commands).is_ok_and(|replies| replies.iter().all(|r| matches!(r, ipc::Reply::Ok(_))))
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
            for px in bmp.pixels.as_chunks::<4>().0 {
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
            let worker = text::TextWorker::new(PathBuf::from(path), Default::default(), || {});
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
                            .find(|&(s, e)| {
                                tt.span_rects(s, e).iter().any(|lr| lr.y0 <= rect.center_y() && lr.y1 >= rect.center_y())
                            })
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
