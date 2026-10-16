use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

use themr::*;

thread_local! {
    // which theme the color editor shows; survives window rebuilds
    static EDIT_THEME: RefCell<String> = RefCell::new("tokyonight-moon".into());
    // last message shown in the status label, survives rebuilds
    static PENDING_STATUS: RefCell<String> = const { RefCell::new(String::new()) };
    static STATUS: RefCell<Option<gtk::Label>> = const { RefCell::new(None) };
}

fn set_status(text: &str) {
    eprintln!("{text}");
    PENDING_STATUS.with(|p| *p.borrow_mut() = text.to_string());
    STATUS.with(|s| {
        if let Some(l) = s.borrow().as_ref() {
            l.set_text(text);
        }
    });
}

fn main() {
    if let Err(e) = first_run() {
        eprintln!("themr setup failed: {e}");
    }
    let app = gtk::Application::builder()
        .application_id("dev.themr.Themr")
        .build();
    app.connect_activate(|app| {
        // The color swatch is natively a wide pill (40x24). Pin it square (40px
        // is its natural width) and round the corners a little. The user's theme
        // also squares off a dropdown's left corners (`dropdown button.toggle`
        // gets `0 9px 9px 0`), so round those too. Priority must beat the user's
        // ~/.config/gtk-4.0/gtk.css, which GTK loads at USER priority.
        let css = gtk::CssProvider::new();
        css.load_from_data(
            "button.color > colorswatch:only-child,\n\
             button.color > colorswatch:only-child > overlay {\n\
                 min-width: 40px;\n\
                 min-height: 40px;\n\
                 border-radius: 12px;\n\
             }\n\
             dropdown > button,\n\
             dropdown > button.toggle {\n\
                 border-radius: 12px;\n\
             }",
        );

        let win = gtk::ApplicationWindow::builder()
            .application(app)
            .title("themr")
            .default_width(780)
            .default_height(700)
            .build();
        gtk::style_context_add_provider_for_display(
            &gtk::prelude::WidgetExt::display(&win),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );

        let header = gtk::HeaderBar::new();
        let import_btn = gtk::Button::with_label("Import JSON…");
        let new_btn = gtk::Button::with_label("New theme");
        let add_btn = gtk::Button::with_label("Add app");
        header.pack_start(&import_btn);
        header.pack_start(&new_btn);
        header.pack_start(&add_btn);
        win.set_titlebar(Some(&header));

        {
            let win = win.clone();
            new_btn.connect_clicked(move |_| open_new_theme_dialog(&win));
        }
        {
            let win = win.clone();
            add_btn.connect_clicked(move |_| open_add_dialog(&win));
        }
        {
            let win = win.clone();
            import_btn.connect_clicked(move |_| open_import_dialog(&win));
        }

        rebuild(&win);
        win.present();
    });
    let _ = app.run();
}

fn rebuild_later(win: &gtk::ApplicationWindow) {
    let win = win.clone();
    glib::idle_add_local_once(move || rebuild(&win));
}

fn load_all() -> (ThemrConfig, BTreeMap<String, Theme>, Vec<String>) {
    let cfg = load_config().unwrap_or_else(|e| {
        eprintln!("{e}");
        ThemrConfig::default()
    });
    let (themes, warnings) = load_themes();
    for w in warnings {
        eprintln!("theme warning: {w}");
    }
    let ids: Vec<String> = themes.keys().cloned().collect();
    (cfg, themes, ids)
}

fn theme_labels(themes: &BTreeMap<String, Theme>, ids: &[String]) -> Vec<String> {
    ids.iter()
        .map(|id| {
            let t = &themes[id];
            if t.name == *id {
                id.clone()
            } else {
                format!("{} ({id})", t.name)
            }
        })
        .collect()
}

fn run_apply(name: Option<&str>) -> String {
    let (cfg, themes, _) = load_all();
    let mut state = load_state().unwrap_or_default();
    let reports = match name {
        Some(n) => vec![apply_one(n, &cfg, &themes, &mut state)],
        None => {
            let names: Vec<String> = cfg.apps.keys().cloned().collect();
            names
                .iter()
                .map(|n| apply_one(n, &cfg, &themes, &mut state))
                .collect()
        }
    };
    if let Err(e) = save_state(&state) {
        eprintln!("{e}");
    }
    match name {
        Some(_) => summarize_app(&reports[0]),
        None => summarize(&reports),
    }
}

fn apply_one(
    name: &str,
    cfg: &ThemrConfig,
    themes: &BTreeMap<String, Theme>,
    state: &mut State,
) -> AppReport {
    let report = apply_app(name, cfg, themes, state);
    for f in &report.files {
        eprintln!("{}", f.label());
        for w in &f.warnings {
            eprintln!("  warning: {w}");
        }
    }
    for e in &report.errors {
        eprintln!("  error: {e}");
    }
    report
}

fn summarize(reports: &[AppReport]) -> String {
    let files: usize = reports.iter().map(|r| r.files.len()).sum();
    let updated: usize = reports
        .iter()
        .map(|r| {
            r.files
                .iter()
                .filter(|f| f.status == FileStatus::Updated)
                .count()
        })
        .sum();
    let colors: usize = reports
        .iter()
        .map(|r| r.files.iter().map(|f| f.replaced).sum::<usize>())
        .sum();
    let adopted: usize = reports
        .iter()
        .map(|r| r.files.iter().map(|f| f.adopted).sum::<usize>())
        .sum();
    let errors: usize = reports.iter().map(|r| r.errors.len()).sum();
    let warnings: usize = reports
        .iter()
        .map(|r| r.files.iter().map(|f| f.warnings.len()).sum::<usize>())
        .sum();
    if reports.is_empty() {
        return "no apps configured yet — press 'Add app'".into();
    }
    if updated == 0 && colors == 0 && adopted == 0 {
        return format!(
            "nothing to change in {files} file(s): no {{{{token}}}} markers and no hex matching this theme's palette — terminal says which file"
        );
    }
    format!(
        "applied: {} app(s), {files} file(s), {updated} rewritten, {colors} colors, {adopted} adopted — {warnings} warning(s), {errors} error(s), details in terminal",
        reports.len()
    )
}

fn summarize_app(report: &AppReport) -> String {
    let updated: usize = report
        .files
        .iter()
        .filter(|f| f.status == FileStatus::Updated)
        .count();
    let colors: usize = report.files.iter().map(|f| f.replaced).sum();
    let adopted: usize = report.files.iter().map(|f| f.adopted).sum();
    let warnings: usize = report.files.iter().map(|f| f.warnings.len()).sum();
    if updated == 0 && colors == 0 && adopted == 0 {
        return format!(
            "{} → '{}': nothing to change (no markers, no palette-matching hex) — terminal says which file",
            report.app, report.theme
        );
    }
    if colors == 0 && adopted > 0 {
        return format!(
            "{} → '{}': {adopted} colors adopted from this theme — press Apply again after switching theme",
            report.app, report.theme
        );
    }
    format!(
        "{} → '{}': {updated} file(s) rewritten, {colors} colors, {adopted} adopted, {warnings} warning(s)",
        report.app, report.theme
    )
}

fn rebuild(win: &gtk::ApplicationWindow) {
    let (cfg, themes, ids) = load_all();
    let labels = theme_labels(&themes, &ids);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(14);
    content.set_margin_end(14);

    // --- global theme row -------------------------------------------------
    let global_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let gl = gtk::Label::new(Some("Theme for all apps"));
    gl.set_xalign(0.0);
    let glabels: Vec<String> = if labels.is_empty() {
        vec!["(no themes)".into()]
    } else {
        labels.clone()
    };
    let global_dd =
        gtk::DropDown::from_strings(&glabels.iter().map(String::as_str).collect::<Vec<_>>());
    global_dd.set_selected(ids.iter().position(|i| *i == cfg.theme).unwrap_or(0) as u32);
    let apply_all = gtk::Button::with_label("Apply to all apps");
    apply_all.add_css_class("suggested-action");
    global_row.append(&gl);
    global_row.append(&global_dd);
    global_row.append(&apply_all);

    {
        let win = win.clone();
        let ids = ids.clone();
        global_dd.connect_selected_notify(move |dd| {
            let Some(id) = ids.get(dd.selected() as usize) else {
                return;
            };
            let mut cfg = load_config().unwrap_or_default();
            cfg.theme = id.clone();
            if let Err(e) = save_config(&cfg) {
                eprintln!("{e}");
                return;
            }
            set_status(&format!("global theme set to '{id}' — Apply to push it"));
            rebuild_later(&win);
        });
    }
    {
        let win = win.clone();
        apply_all.connect_clicked(move |_| {
            let msg = run_apply(None);
            set_status(&msg);
            let _ = &win;
        });
    }

    content.append(&global_row);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // --- app rows ---------------------------------------------------------
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("background");

    for (app_name, app_cfg) in &cfg.apps {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        row.set_margin_start(8);
        row.set_margin_end(8);

        let name_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        name_box.set_hexpand(true);
        let name_lbl = gtk::Label::new(Some(app_name));
        name_lbl.set_markup(&format!("<b>{app_name}</b>"));
        name_lbl.set_xalign(0.0);
        let mut shown = app_cfg.files.clone();
        shown.truncate(1);
        let files_lbl = gtk::Label::new(Some(&shown.join(", ")));
        files_lbl.set_xalign(0.0);
        files_lbl.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        files_lbl.set_opacity(0.6);
        let files_note = gtk::Label::new(Some(&format!(
            "{} file(s) · effective theme: {}",
            app_cfg.files.len(),
            cfg.app_theme(app_name)
        )));
        files_note.set_xalign(0.0);
        files_note.set_opacity(0.6);
        name_box.append(&name_lbl);
        name_box.append(&files_lbl);
        name_box.append(&files_note);

        // per-app theme dropdown: index 0 = follow the global theme
        let mut opt_labels = vec![format!("Global ({})", cfg.theme)];
        opt_labels.extend(labels.iter().cloned());
        let dd =
            gtk::DropDown::from_strings(&opt_labels.iter().map(String::as_str).collect::<Vec<_>>());
        dd.set_selected(
            app_cfg
                .theme
                .as_ref()
                .and_then(|t| ids.iter().position(|i| i == t))
                .map(|i| i as u32 + 1)
                .unwrap_or(0),
        );

        let apply_btn = gtk::Button::with_label("Apply");
        let restore_btn = gtk::Button::with_label("Restore");
        restore_btn.set_tooltip_text(Some("Put files back the way they were before themr"));
        let remove_btn = gtk::Button::with_label("✕");
        remove_btn.add_css_class("destructive-action");
        remove_btn.set_tooltip_text(Some("Remove from themr (config files are kept)"));

        {
            let ids = ids.clone();
            let name = app_name.clone();
            dd.connect_selected_notify(move |dd| {
                let idx = dd.selected();
                let mut cfg = load_config().unwrap_or_default();
                if let Some(app) = cfg.apps.get_mut(name.as_str()) {
                    app.theme = if idx == 0 {
                        None
                    } else {
                        ids.get(idx as usize - 1).cloned()
                    };
                    match save_config(&cfg) {
                        Ok(()) => {
                            let t = cfg.app_theme(&name);
                            set_status(&format!("{name} now uses '{t}' — press Apply on the row"));
                        }
                        Err(e) => eprintln!("{e}"),
                    }
                }
            });
        }
        {
            let name = app_name.clone();
            apply_btn.connect_clicked(move |_| {
                let msg = run_apply(Some(&name));
                set_status(&msg);
            });
        }
        {
            let name = app_name.clone();
            restore_btn.connect_clicked(move |_| {
                let cfg = load_config().unwrap_or_default();
                let mut state = load_state().unwrap_or_default();
                let msgs = restore_app(&name, &cfg, &mut state);
                if let Err(e) = save_state(&state) {
                    eprintln!("{e}");
                }
                set_status(&msgs.join(" · "));
            });
        }
        {
            let name = app_name.clone();
            let win = win.clone();
            remove_btn.connect_clicked(move |_| {
                let mut cfg = load_config().unwrap_or_default();
                cfg.apps.remove(name.as_str());
                if let Err(e) = save_config(&cfg) {
                    eprintln!("{e}");
                    return;
                }
                set_status(&format!("removed '{name}' from themr"));
                rebuild_later(&win);
            });
        }

        row.append(&name_box);
        row.append(&dd);
        row.append(&apply_btn);
        row.append(&restore_btn);
        row.append(&remove_btn);

        let list_row = gtk::ListBoxRow::new();
        list_row.set_child(Some(&row));
        list.append(&list_row);
    }

    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    content.append(&scroll);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // --- color editor -----------------------------------------------------
    let edit_id = EDIT_THEME.with(|t| t.borrow().clone());
    let edit_id = if themes.contains_key(&edit_id) {
        edit_id
    } else {
        cfg.theme.clone()
    };

    let edit_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let eh = gtk::Label::new(Some("Editing colors of"));
    eh.set_xalign(0.0);
    let mut edit_labels = labels.clone();
    if edit_labels.is_empty() {
        edit_labels.push("(no themes)".into());
    }
    let edit_dd =
        gtk::DropDown::from_strings(&edit_labels.iter().map(String::as_str).collect::<Vec<_>>());
    edit_dd.set_selected(ids.iter().position(|i| *i == edit_id).unwrap_or(0) as u32);
    let hint = gtk::Label::new(Some(
        "— edits save instantly, Apply buttons push them to the apps",
    ));
    hint.set_opacity(0.6);
    edit_head.append(&eh);
    edit_head.append(&edit_dd);
    edit_head.append(&hint);

    {
        let win = win.clone();
        let ids = ids.clone();
        edit_dd.connect_selected_notify(move |dd| {
            let Some(id) = ids.get(dd.selected() as usize) else {
                return;
            };
            EDIT_THEME.with(|t| *t.borrow_mut() = id.clone());
            rebuild_later(&win);
        });
    }

    let grid = gtk::FlowBox::new();
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid.set_max_children_per_line(8);
    grid.set_min_children_per_line(2);
    grid.set_homogeneous(false);
    grid.set_column_spacing(12);
    grid.set_row_spacing(10);

    if let Some(t) = themes.get(&edit_id) {
        for (token, hex) in &t.colors {
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 3);
            cell.set_halign(gtk::Align::Center);
            let button = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
            button.set_halign(gtk::Align::Center); // same square size for every token
            if let Some((r, g, b, a)) = hex_to_rgba(hex) {
                button.set_rgba(&gdk::RGBA::new(r, g, b, a));
            }
            let lbl = gtk::Label::new(Some(token));
            lbl.set_opacity(0.7);
            cell.append(&button);
            cell.append(&lbl);
            grid.insert(&cell, -1);

            let token = token.clone();
            let id = edit_id.clone();
            button.connect_rgba_notify(move |btn| {
                let rgba = btn.rgba();
                let hex = rgba_to_hex(rgba.red(), rgba.green(), rgba.blue(), rgba.alpha());
                let path = themes_dir().join(format!("{id}.json"));
                let Ok(text) = fs::read_to_string(&path) else {
                    return;
                };
                let Ok(mut t) = parse_theme(&text, &id) else {
                    return;
                };
                t.colors.insert(token.clone(), hex);
                match serde_json::to_string_pretty(&t) {
                    Ok(out) => {
                        if let Err(e) = fs::write(&path, out) {
                            eprintln!("{e}");
                        }
                    }
                    Err(e) => eprintln!("{e}"),
                }
            });
        }
    } else {
        let empty = gtk::Label::new(Some("No themes found in ~/.config/themr/themes"));
        empty.set_opacity(0.6);
        grid.insert(&empty, -1);
    }
    content.append(&edit_head);
    content.append(&grid);

    // --- status -----------------------------------------------------------
    let msg = PENDING_STATUS.with(|p| p.borrow().clone());
    let status = gtk::Label::new(Some(if msg.is_empty() {
        "Ready. Pick a theme, then Apply."
    } else {
        &msg
    }));
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_opacity(0.8);
    status.set_margin_top(4);
    STATUS.with(|s| *s.borrow_mut() = Some(status.clone()));
    content.append(&status);

    win.set_child(Some(&content));
}

// ---------- dialogs -------------------------------------------------------

fn open_import_dialog(win: &gtk::ApplicationWindow) {
    let dialog = gtk::FileDialog::builder()
        .title("Import community theme (JSON)")
        .modal(true)
        .build();
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("JSON themes"));
    filter.add_pattern("*.json");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));

    let parent = win.clone();
    let win = win.clone();
    dialog.open(Some(&parent), gtk::gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        match import_from_path(&path) {
            Ok(id) => {
                EDIT_THEME.with(|t| *t.borrow_mut() = id.clone());
                set_status(&format!(
                    "imported theme '{id}' — edit its colors below, Apply to use it"
                ));
                rebuild_later(&win);
            }
            Err(e) => set_status(&format!("import failed: {e}")),
        }
    });
}

fn import_from_path(path: &PathBuf) -> Result<String, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("theme");
    import_theme(&text, stem)
}

fn open_add_dialog(win: &gtk::ApplicationWindow) {
    let dialog = gtk::Window::builder()
        .title("Add app")
        .transient_for(win)
        .modal(true)
        .default_width(480)
        .build();

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    vbox.set_margin_top(14);
    vbox.set_margin_bottom(14);
    vbox.set_margin_start(14);
    vbox.set_margin_end(14);

    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some("app name, e.g. foot"));

    let hint = gtk::Label::new(Some("Config files to theme, one per line (~ works):"));
    hint.set_xalign(0.0);
    hint.set_opacity(0.7);

    let files_view = gtk::TextView::new();
    files_view.set_vexpand(true);
    files_view.set_monospace(true);
    files_view.buffer().set_text("~/.config/foot/foot.ini\n");
    let scroll = gtk::ScrolledWindow::builder()
        .child(&files_view)
        .min_content_height(120)
        .vexpand(true)
        .build();

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let add = gtk::Button::with_label("Add");
    add.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&add);

    vbox.append(&name_entry);
    vbox.append(&hint);
    vbox.append(&scroll);
    vbox.append(&buttons);
    dialog.set_child(Some(&vbox));

    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| dialog.close());
    }
    {
        let dialog = dialog.clone();
        let win = win.clone();
        add.connect_clicked(move |_| {
            let name = name_entry.text().to_string();
            let name = name.trim().to_string();
            let buf = files_view.buffer();
            let (s, e) = (buf.start_iter(), buf.end_iter());
            let files: Vec<String> = buf
                .text(&s, &e, true)
                .to_string()
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if name.is_empty() || files.is_empty() {
                set_status("Add app: a name and at least one file path are required");
                return;
            }
            let mut cfg = load_config().unwrap_or_default();
            cfg.apps
                .insert(name.clone(), AppConfig { theme: None, files });
            if let Err(e) = save_config(&cfg) {
                eprintln!("{e}");
                return;
            }
            set_status(&format!(
                "added app '{name}' — press Apply: themr adopts any colors already matching the theme"
            ));
            dialog.close();
            rebuild_later(&win);
        });
    }

    dialog.present();
}

fn open_new_theme_dialog(win: &gtk::ApplicationWindow) {
    let (_, themes, _) = load_all();
    let edit_id = EDIT_THEME.with(|t| t.borrow().clone());
    let base_id = if themes.contains_key(&edit_id) {
        edit_id
    } else if let Some(first) = themes.keys().next() {
        first.clone()
    } else {
        set_status("cannot create a theme: no themes found in ~/.config/themr/themes");
        return;
    };

    let dialog = gtk::Window::builder()
        .title("New theme")
        .transient_for(win)
        .modal(true)
        .default_width(400)
        .build();

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    vbox.set_margin_top(14);
    vbox.set_margin_bottom(14);
    vbox.set_margin_start(14);
    vbox.set_margin_end(14);

    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some("theme name, e.g. my-dark"));

    let hint = gtk::Label::new(Some(&format!(
        "Starts as a copy of '{base_id}'. Edit its colors afterwards."
    )));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    hint.set_opacity(0.7);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let create = gtk::Button::with_label("Create");
    create.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&create);

    vbox.append(&name_entry);
    vbox.append(&hint);
    vbox.append(&buttons);
    dialog.set_child(Some(&vbox));

    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| dialog.close());
    }
    {
        let dialog = dialog.clone();
        let win = win.clone();
        let base = themes[&base_id].clone();
        create.connect_clicked(move |_| {
            let name = name_entry.text().trim().to_string();
            if name.is_empty() {
                set_status("New theme: a name is required");
                return;
            }
            match duplicate_theme(&base, &name) {
                Ok(id) => {
                    EDIT_THEME.with(|t| *t.borrow_mut() = id.clone());
                    set_status(&format!(
                        "created theme '{name}' ({id}) — edit its colors below, then Apply"
                    ));
                    dialog.close();
                    rebuild_later(&win);
                }
                Err(e) => set_status(&format!("could not create theme: {e}")),
            }
        });
    }

    dialog.present();
}
