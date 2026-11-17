use std::time::Duration;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use muzoryn::{
    app::{App, InputMode, View},
    config::Settings,
    core::{Command, Playback, Snapshot},
    keys::Action,
    library::Library,
    theme::Theme,
    ui,
};

fn app() -> App {
    let lib = Library::open_memory().unwrap();
    lib.insert_simple("/m/time.flac", "Time", "Pink Floyd", "The Dark Side of the Moon", "Rock", 225).unwrap();
    lib.insert_simple("/m/money.flac", "Money", "Pink Floyd", "The Dark Side of the Moon", "Rock", 383).unwrap();
    lib.insert_simple("/m/nirvana.flac", "Lithium", "Nirvana", "Nevermind", "Rock", 257).unwrap();
    let mut settings = Settings::default();
    settings.podcasts.clear(); // sin red en las pruebas
    let mut app = App::new(settings, lib, None, "nocturne".into(), Theme::default(), false);
    app.keys_help = "Espacio           Pausa / reanudar".into();
    let items: Vec<_> = app.tracks.iter().map(|t| t.to_item()).collect();
    let mut snap = Snapshot::new(items);
    snap.current = Some(0);
    snap.playback = Playback::Playing;
    snap.duration = Some(Duration::from_secs(225));
    snap.position = Duration::from_secs(81);
    snap.spectrum = (0..48).map(|i| i as f32 / 48.0).collect();
    app.update(snap);
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..h {
        for x in 0..w { text.push_str(buffer[(x, y)].symbol()); }
        text.push('\n');
    }
    text
}

fn press(app: &mut App, c: char) -> Vec<Command> {
    app.handle_text(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

#[test]
fn every_view_renders_at_every_size() {
    let mut app = app();
    for view in View::ALL {
        app.set_view(view);
        app.input = None;
        for (w, h) in [(160, 48), (120, 34), (100, 30), (80, 24), (70, 20), (40, 12), (1, 1)] {
            let text = draw(&mut app, w, h);
            if w >= 100 && h >= 30 {
                assert!(text.contains(view.title()), "falta el título de {view:?} a {w}x{h}");
            }
        }
    }
}

#[test]
fn wide_layout_shows_sidebar_and_player() {
    let mut app = app();
    let text = draw(&mut app, 140, 40);
    for label in ["Inicio", "Biblioteca", "Búsqueda", "Cola", "Listas", "Radio", "Podcasts", "Letras", "Ajustes"] {
        assert!(text.contains(label), "falta {label}");
    }
    assert!(text.contains("MUZORYN"));
    assert!(text.contains("Time"));
    assert!(text.contains("1:21"));
    assert!(text.contains("3:45"));
}

#[test]
fn small_terminal_shows_notice() {
    let mut app = app();
    assert!(draw(&mut app, 50, 14).contains("Amplía la terminal"));
}

#[test]
fn control_characters_are_removed() {
    assert_eq!(ui::safe_text("a\x1b[31m\nb"), "a[31mb");
}

#[test]
fn help_is_rendered_as_overlay() {
    let mut app = app();
    app.action(Action::Help);
    assert!(draw(&mut app, 120, 34).contains("Ayuda"));
    app.action(Action::Cancel);
    assert!(!app.help);
}

#[test]
fn number_keys_switch_views() {
    let mut app = app();
    app.action(Action::View5);
    assert_eq!(app.view, View::Albumes);
    app.action(Action::View10);
    assert_eq!(app.view, View::Audio);
    app.action(Action::View12);
    assert_eq!(app.view, View::Ajustes);
}

#[test]
fn library_enter_replaces_queue_with_selection() {
    let mut app = app();
    app.action(Action::View2);
    app.action(Action::Play); // pasa al panel de pistas
    app.action(Action::Down);
    let commands = app.action(Action::Play);
    match commands.as_slice() {
        [Command::Replace { items, start }] => {
            assert_eq!(items.len(), 3);
            assert_eq!(*start, 1);
        }
        other => panic!("comando inesperado: {other:?}"),
    }
}

#[test]
fn search_finds_by_artist_and_plays_track() {
    let mut app = app();
    app.action(Action::View3);
    assert!(app.input_active());
    for c in "pink floyd".chars() { press(&mut app, c); }
    assert_eq!(app.search_total[0], 2);
    assert_eq!(app.search_total[2], 1);
    app.handle_text(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!app.input_active());
    let commands = app.action(Action::Play);
    assert!(matches!(commands.as_slice(), [Command::Play(_)]));
    assert!(draw(&mut app, 120, 34).contains("Canciones (2)"));
}

#[test]
fn favorites_and_playlists_flow() {
    let mut app = app();
    app.action(Action::View2);
    app.action(Action::Play);
    app.action(Action::Favorite);
    assert_eq!(app.favorites.len(), 1);
    app.action(Action::NewList);
    for c in "Viajes".chars() { press(&mut app, c); }
    app.handle_text(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.playlists.len(), 1);
    assert_eq!(app.list_name(), "Viajes");
    app.action(Action::View2);
    app.action(Action::Play);
    app.action(Action::AddToList);
    app.action(Action::View6);
    assert_eq!(app.list_tracks.len(), 1);
    assert!(draw(&mut app, 120, 34).contains("Viajes"));
}

#[test]
fn equalizer_keys_adjust_band() {
    let mut app = app();
    app.action(Action::View10);
    app.action(Action::SeekForward); // banda 1
    let commands = app.action(Action::Up);
    match commands.as_slice() {
        [Command::SetEq(g)] => { assert_eq!(g[1], 1.0); assert_eq!(g[0], 0.0); }
        other => panic!("comando inesperado: {other:?}"),
    }
    let commands = app.action(Action::EqPreset);
    assert!(matches!(commands.as_slice(), [Command::SetEq(_)]));
}

#[test]
fn queue_edit_commands() {
    let mut app = app();
    app.action(Action::View4);
    app.action(Action::Down);
    assert!(matches!(app.action(Action::MoveUp).as_slice(), [Command::MoveUp(1)]));
    assert!(matches!(app.action(Action::Delete).as_slice(), [Command::Remove(0)]));
    assert!(matches!(app.action(Action::ClearQueue).as_slice(), [Command::Clear]));
}

#[test]
fn transport_actions_map_to_commands() {
    let mut app = app();
    assert!(matches!(app.action(Action::Toggle).as_slice(), [Command::Toggle]));
    assert!(matches!(app.action(Action::Shuffle).as_slice(), [Command::ToggleShuffle]));
    assert!(matches!(app.action(Action::SeekForward).as_slice(), [Command::SeekBy(5)]));
    assert!(matches!(app.action(Action::Crossfade).as_slice(), [Command::SetCrossfade(3)]));
}

fn type_text(app: &mut App, text: &str) { for c in text.chars() { press(app, c); } }
fn enter(app: &mut App) { app.handle_text(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); }

#[test]
fn add_station_asks_url_then_name() {
    let mut app = app();
    let before = app.settings.stations.len();
    app.action(Action::View7);
    app.action(Action::Enqueue);
    assert_eq!(app.input.as_ref().map(|i| i.mode), Some(InputMode::AddStation));
    type_text(&mut app, "radio.example.com/live");
    enter(&mut app);
    assert_eq!(app.input.as_ref().map(|i| i.mode), Some(InputMode::NameStation));
    assert!(draw(&mut app, 100, 30).contains("Nombre de la emisora"));
    app.handle_text(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.input.is_none());
    assert_eq!(app.settings.stations.len(), before);
}

#[test]
fn invalid_url_is_rejected_and_podcast_flow_starts() {
    let mut app = app();
    app.action(Action::View7);
    app.action(Action::AddSource);
    type_text(&mut app, "ftp://x.test");
    enter(&mut app);
    assert!(app.input.is_none());
    app.action(Action::View8);
    app.action(Action::AddSource);
    assert_eq!(app.input.as_ref().map(|i| i.mode), Some(InputMode::AddPodcast));
}

#[test]
fn removing_a_station_needs_confirmation() {
    let mut app = app();
    let before = app.settings.stations.len();
    app.action(Action::View7);
    app.action(Action::Delete);
    assert_eq!(app.settings.stations.len(), before);
    app.action(Action::Delete);
    assert_eq!(app.settings.stations.len(), before - 1);
}

#[test]
fn detach_keeps_daemon_and_quit_all_stops_it() {
    let mut a = app();
    a.action(Action::Detach);
    assert!(a.quit && !a.quit_all);
    let mut b = app();
    b.action(Action::QuitAll);
    assert!(b.quit && b.quit_all);
}

#[test]
fn reopening_restores_view_and_panel() {
    let mut a = app();
    a.action(Action::View2);
    a.action(Action::Play); // panel de pistas
    assert_eq!((a.view, a.focus), (View::Biblioteca, 1));
    a.save_settings();
    let saved = a.settings.clone();
    let lib = Library::open_memory().unwrap();
    let mut b = App::new(saved, lib, None, "nocturne".into(), Theme::default(), true);
    b.restore_view();
    assert_eq!((b.view, b.focus), (View::Biblioteca, 1));
}

#[test]
fn library_folder_tree_navigates_and_enqueues() {
    use std::path::PathBuf;
    let lib = Library::open_memory().unwrap();
    for (p, t, ar, al) in [
        ("/m/Jazz/Nina Simone/Pastel Blues/01.flac", "Be My Husband", "Nina Simone", "Pastel Blues"),
        ("/m/Jazz/Nina Simone/Pastel Blues/02.flac", "Sinnerman", "Nina Simone", "Pastel Blues"),
        ("/m/Jazz/Louis Armstrong/Hot Fives/01.flac", "Heebie Jeebies", "Louis Armstrong", "Hot Fives"),
        ("/m/Rock/Nirvana/Nevermind/01.flac", "Lithium", "Nirvana", "Nevermind"),
    ] { lib.insert_simple(p, t, ar, al, "x", 100).unwrap(); }
    let mut settings = Settings::default();
    settings.music_dirs = vec![PathBuf::from("/m")];
    settings.podcasts.clear();
    let mut a = App::new(settings, lib, None, "nocturne".into(), Theme::default(), false);
    a.action(Action::View2);
    let names = |a: &App| a.folders.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&a), ["Todo", "m", "Jazz", "Rock"]); // raíz desplegada, resto plegado
    a.action(Action::Down); a.action(Action::Down); // Jazz
    a.action(Action::SeekForward); // desplegar
    assert_eq!(names(&a), ["Todo", "m", "Jazz", "Louis Armstrong", "Nina Simone", "Rock"]);
    a.action(Action::Down); a.action(Action::Down); // Nina Simone (solo ella)
    assert_eq!(a.lib_rows.len(), 2);
    assert!(!a.action(Action::Enqueue).is_empty()); // carpeta entera a la cola
    a.action(Action::SeekBackward); // plegada/sin hijos → sube a Jazz
    assert_eq!(a.folders[a.folder_cur.index].name, "Jazz");
    assert_eq!(a.lib_rows.len(), 3);
}

#[test]
fn station_favorites_and_sections() {
    let mut app = app();
    app.action(Action::View7);
    assert!(app.action(Action::Favorite).is_empty());
    assert!(app.settings.stations[0].fav);
    app.action(Action::FocusNext); // Favoritas
    assert_eq!(app.radio_visible(), vec![0]);
    assert!(draw(&mut app, 120, 34).contains('★'));
    app.action(Action::Favorite); // la quita desde la propia sección
    assert!(!app.settings.stations[0].fav);
    assert!(app.radio_visible().is_empty());
    assert!(draw(&mut app, 120, 34).contains("Aún no hay favoritas"));
    app.action(Action::FocusNext); // Recientes
    app.action(Action::FocusNext); // Explorar Radio Browser
    assert!(app.radio_browser_mode);
    app.action(Action::FocusNext); // vuelve a Mis emisoras
    assert!(!app.radio_browser_mode);
    assert_eq!(app.radio_visible().len(), app.settings.stations.len());
}

#[test]
fn recent_stations_keep_order_and_can_be_cleared_without_deleting() {
    let mut app = app();
    let total = app.settings.stations.len();
    app.action(Action::View7);
    assert!(matches!(app.action(Action::Play).as_slice(), [Command::Play(item)] if item.live));
    app.action(Action::Down);
    app.action(Action::Play);
    app.action(Action::FocusNext); // Favoritas
    app.action(Action::FocusNext); // Recientes
    assert_eq!(app.radio_visible(), vec![1, 0]);
    app.action(Action::Delete); // quita solo del historial
    assert_eq!(app.radio_visible(), vec![0]);
    assert_eq!(app.settings.stations.len(), total);
}

#[test]
fn playing_station_can_be_favorited_from_any_view() {
    let mut app = app();
    let url = app.settings.stations[1].url.clone();
    let mut snap = Snapshot::new(vec![muzoryn::media::QueueItem::stream("Radio", "Jazz", &url, true)]);
    snap.current = Some(0);
    snap.playback = Playback::Playing;
    app.update(snap);
    app.action(Action::View1);
    app.action(Action::Favorite);
    assert!(app.settings.stations[1].fav);
}

#[test]
fn live_player_shows_song_and_connection_state() {
    use muzoryn::core::StreamState;
    let mut app = app();
    let mut snap = Snapshot::new(vec![muzoryn::media::QueueItem::stream("Jazz FM", "jazz", "https://x.test/s", true)]);
    snap.current = Some(0);
    snap.playback = Playback::Playing;
    snap.stream = StreamState::Live;
    snap.now_playing = Some("Miles Davis - So What".into());
    app.update(snap.clone());
    let text = draw(&mut app, 140, 40);
    assert!(text.contains("Miles Davis - So What") && text.contains("En vivo"));
    snap.now_playing = None;
    snap.stream = StreamState::Reconnecting;
    app.update(snap.clone());
    let text = draw(&mut app, 140, 40);
    assert!(text.contains("Jazz FM") && text.contains("Reconectando"));
    snap.stream = StreamState::Unavailable;
    app.update(snap);
    assert!(draw(&mut app, 140, 40).contains("Emisora no disponible"));
}

fn show(title: &str, feed: &str) -> muzoryn::podcast_directory::ShowResult {
    muzoryn::podcast_directory::ShowResult {
        title: title.into(), author: "Autora".into(), feed_url: feed.into(), genre: "Noticias".into(), episodes: 120,
    }
}

#[test]
fn podcast_explorer_lists_subscribes_and_navigates_back() {
    use muzoryn::app::PodcastSection;
    let mut app = app();
    app.action(Action::View8);
    app.podcast_browser = true;
    app.podcast_section = PodcastSection::Popular;
    app.shows = vec![show("Radio Ambulante", "https://f.test/ra.xml"), show("Hoy en la historia", "https://f.test/h.xml")];
    let text = draw(&mut app, 120, 34);
    assert!(text.contains("Radio Ambulante") && text.contains("Podcasts más escuchados") && text.contains("Populares"));
    let before = app.settings.podcasts.len();
    app.action(Action::Favorite);
    assert_eq!(app.settings.podcasts.len(), before + 1);
    assert!(draw(&mut app, 120, 34).contains('✓'));
    app.action(Action::Favorite);
    assert_eq!(app.settings.podcasts.len(), before);

    app.action(Action::FocusNext); // Categorías
    assert_eq!(app.podcast_section, PodcastSection::Categories);
    let text = draw(&mut app, 120, 34);
    assert!(text.contains("Noticias") && text.contains("Crimen real") && text.contains("Religión y espiritualidad"));
    app.category_open = true; // dentro de una categoría, Esc vuelve a la lista de categorías
    app.action(Action::Cancel);
    assert!(app.podcast_browser && !app.category_open);
    app.action(Action::Cancel); // y otra vez sale del explorador
    assert!(!app.podcast_browser);
}

#[test]
fn opening_an_already_subscribed_show_selects_it() {
    use muzoryn::app::PodcastSection;
    let mut app = app();
    app.action(Action::View8);
    app.settings.podcasts.push(muzoryn::config::PodcastFeed { name: "Hoy en la historia".into(), url: "https://f.test/h.xml".into() });
    let index = app.settings.podcasts.len() - 1;
    app.episodes.insert(index, Vec::new());
    app.podcast_browser = true;
    app.podcast_section = PodcastSection::Results;
    app.shows = vec![show("Hoy en la historia", "https://f.test/h.xml")];
    assert!(app.action(Action::Play).is_empty());
    assert!(!app.podcast_browser);
    assert_eq!(app.podcast_cur.index, index);
    assert_eq!(app.focus, 1);
}

#[test]
fn download_channel_asks_for_confirmation_first() {
    let mut app = app();
    app.action(Action::View8);
    app.settings.podcasts.push(muzoryn::config::PodcastFeed { name: "Canal de prueba".into(), url: "https://f.test/c.xml".into() });
    let index = app.settings.podcasts.len() - 1;
    app.podcast_cur.index = index;
    app.episodes.insert(index, vec![
        muzoryn::feeds::Episode { title: "Zzz episodio uno 7391".into(), url: "https://f.test/1.mp3".into(), ..Default::default() },
        muzoryn::feeds::Episode { title: "Zzz episodio dos 7391".into(), url: "https://f.test/2.mp3".into(), ..Default::default() },
    ]);
    assert!(app.action(Action::DownloadAll).is_empty());
    assert!(app.bulk.is_none(), "la primera pulsación solo pide confirmar");
    let text = draw(&mut app, 140, 34);
    assert!(text.contains("Pulsa D otra vez") && text.contains("2 episodios"));
    assert!(text.contains("canal completo"));
}


#[test]
fn podcasts_context_exposes_feed_refresh_shortcut() {
    let mut app = app();
    app.set_view(View::Podcasts);
    let text = draw(&mut app, 120, 34);
    assert!(text.contains("actualizar feed"));
    assert!(text.contains("Pulsa u para cargar el feed"));
}

#[test]
fn podcast_browser_exposes_popular_refresh_shortcut() {
    let mut app = app();
    app.set_view(View::Podcasts);
    app.podcast_browser = true;
    app.podcast_section = muzoryn::app::PodcastSection::Popular;
    let text = draw(&mut app, 120, 34);
    assert!(text.contains("recargar populares"));
}
