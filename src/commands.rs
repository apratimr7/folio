//! The command registry. Every action in folio is a named command taking an
//! optional argument string; key bindings, the command palette, Lua scripts
//! (and later AI tools) all go through this one layer.

pub struct BuiltinCommand {
    pub name: &'static str,
    pub description: &'static str,
    /// Human hint for the argument, if the command takes one.
    pub arg: Option<&'static str>,
}

macro_rules! commands {
    ($( $name:literal $(($arg:literal))? => $desc:literal ),* $(,)?) => {
        pub const BUILTINS: &[BuiltinCommand] = &[
            $( BuiltinCommand { name: $name, description: $desc, arg: commands!(@arg $($arg)?) } ),*
        ];
    };
    (@arg $arg:literal) => { Some($arg) };
    (@arg) => { None };
}

commands! {
    "move_down" => "Scroll down a little (count: repeat)",
    "move_up" => "Scroll up a little",
    "move_left" => "Scroll left",
    "move_right" => "Scroll right",
    "half_page_down" => "Scroll down half a screen",
    "half_page_up" => "Scroll up half a screen",
    "screen_down" => "Scroll down one screen",
    "screen_up" => "Scroll up one screen",
    "next_page" => "Go to the next page",
    "prev_page" => "Go to the previous page",
    "goto_start" => "Go to the first page (count: go to page N)",
    "goto_end" => "Go to the last page (count: go to page N)",
    "goto_page" ("page number or label") => "Go to a page",
    "goto_page_prompt" => "Ask for a page number and go there",
    "zoom_in" => "Zoom in",
    "zoom_out" => "Zoom out",
    "zoom" ("percent") => "Set the zoom level",
    "fit_width" => "Fit page width to the window",
    "fit_page" => "Fit the whole page in the window",
    "actual_size" => "Zoom to 100%",
    "center_line" => "Center the current ruler line or match",
    "search" ("text") => "Search the document",
    "search_backward" ("text") => "Search the document, backwards",
    "next_match" => "Next search match",
    "prev_match" => "Previous search match",
    "search_selection" => "Search for the selected text",
    "clear" => "Clear search, selection and overlays",
    "command_palette" => "Open the command palette",
    "toc" => "Fuzzy-find in the table of contents",
    "toggle_sidebar" => "Show or hide the outline sidebar",
    "open_dialog" => "Open a PDF with the system's file dialog",
    "open_file" ("path") => "Open a PDF: type or browse to a path (keyboard file picker)",
    "recent_files" => "Reopen a recent document (Ctrl-D removes one from the list)",
    "alternate_file" => "Switch to the previously opened document",
    "new_tab" ("path") => "Open a document in a new tab (asks with the system dialog without a path)",
    "close_tab" ("tab number") => "Close the current tab (or tab N)",
    "next_tab" => "Go to the next tab (count: that many tabs on)",
    "prev_tab" => "Go to the previous tab",
    "goto_tab" ("tab number") => "Go to tab N (1 is the first)",
    "tabs" => "List open tabs (Ctrl-D closes one)",
    "reopen_tab" => "Reopen the most recently closed tab",
    "reveal_file" => "Show the document in the file manager",
    "copy_path" => "Copy the document's path",
    "set_mark" ("letter") => "Set a mark (A-Z marks are global)",
    "goto_mark" ("letter") => "Jump to a mark",
    "marks" => "List marks (Ctrl-D deletes)",
    "delete_mark" ("letter") => "Delete a mark",
    "delete_marks" => "Delete all marks of this document (not the global A-Z ones)",
    "jump_back" => "Jump back in history",
    "jump_forward" => "Jump forward in history",
    "follow_link" => "Show link hints and follow one",
    "peek_link" => "Show link hints and preview the target",
    "smart_jump" => "Hint references (citations, figures, equations…) and jump",
    "smart_peek" => "Hint references and preview the target in place",
    "toggle_dark" => "Toggle dark page colors",
    "toggle_custom_colors" => "Toggle theme-tinted page colors",
    "page_mode" ("normal|dark|custom") => "Set the page color mode",
    "cycle_theme" => "Switch to the next UI theme",
    "theme" ("name") => "Set the UI theme",
    "add_bookmark" ("label") => "Bookmark the current position",
    "bookmarks" => "List bookmarks",
    "visual" => "Start a selection: pick a line, then a word (v again: first word on screen)",
    "exit_visual" => "Leave selection mode",
    "sel_word_next" => "Extend selection by a word",
    "sel_word_prev" => "Shrink/extend selection a word back",
    "sel_char_next" => "Extend selection by a character",
    "sel_char_prev" => "Move selection end back a character",
    "sel_line_next" => "Extend selection a line down",
    "sel_line_prev" => "Move selection end a line up",
    "sel_line_end" => "Extend selection to the end of the line",
    "sel_line_start" => "Move selection end to the line start",
    "sel_sentence_next" => "Extend selection to the end of the sentence",
    "sel_swap" => "Swap selection anchor and cursor",
    "yank" => "Copy the selection",
    "yank_page" => "Copy the text of the current page",
    "highlight" ("color") => "Highlight the selection",
    "annotate" ("note") => "Highlight the selection with a note",
    "highlights" => "List highlights (Ctrl-E edits, Ctrl-D deletes)",
    "edit_highlight" => "Label the highlights on screen and edit one: color, note, copy, delete",
    "highlight_color" ("number color") => "Change the color of highlight N (as numbered in the list)",
    "highlight_note" ("number text") => "Set the note of highlight N",
    "delete_highlight" ("number") => "Delete highlight N",
    "export_highlights" ("path") => "Copy all highlights and notes as Markdown (with a path: write them to that file)",
    "save_highlights" => "Save highlights and notes as a Markdown file (asks where)",
    "toggle_ruler" => "Toggle the reading ruler (j/k move by line)",
    "reload" => "Reload the document",
    "rotate" ("degrees") => "Rotate the pages clockwise (default 90; -90 turns back)",
    "toggle_statusbar" => "Show or hide the status bar",
    "toggle_text_panel" => "Show or hide the side text panel",
    "fullscreen" => "Toggle fullscreen",
    "focus_window" => "Bring the window to the front",
    "help" => "Show all key bindings",
    "reload_config" => "Reload config.toml and init.lua",
    "lua" ("code") => "Run a Lua snippet",
    "echo" ("text") => "Show a message",
    "feed_keys" ("keys") => "Act as if keys were typed, in vim notation (e.g. gg or oo)",
    "open_url" ("url") => "Open a URL in the default browser",
    "screenshot" ("path") => "Save a PNG of the window (asks where without a path)",
    "quit" => "Quit folio",
    "q" => "Quit folio",
}

pub fn builtin(name: &str) -> Option<&'static BuiltinCommand> {
    BUILTINS.iter().find(|c| c.name == name)
}

/// Split `"goto_page 12"` into `("goto_page", "12")`.
pub fn split(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], line[i..].trim()),
        None => (line, ""),
    }
}
