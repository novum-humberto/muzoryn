# MUZORYN — MPRIS pretest 01 (sin compilar ni probar)
Añade `src/mpris.rs` (zbus 4.4.0 bloqueante), activado con `--features mpris` y desactivado por defecto.
Corre en el daemon (`ipc::serve`), nombre de bus `org.mpris.MediaPlayer2.muzoryn`.

Soporta: Play/Pause/PlayPause/Stop/Next/Previous/Seek/SetPosition, propiedades PlaybackStatus, LoopStatus, Shuffle, Volume, Metadata, Position, CanSeek (no en radio) y señales PropertiesChanged ante cambios. No soporta TrackList, OpenUri ni Rate.

## Pasos en tu Arch (aquí no hay cargo ni red)
```bash
cargo build --release --features mpris   # sin --locked la primera vez: regenera Cargo.lock
cargo test --features mpris
cargo clippy --features mpris -- -D warnings
muzoryn --daemon &  playerctl -p muzoryn status; playerctl -p muzoryn metadata
```
## Riesgos
1. Nunca compilado: la API de zbus (`#[interface]`, `#[zbus(property)]`, `blocking::ConnectionBuilder`, `*_changed`) se escribió de memoria para 4.4.0; pueden hacer falta retoques.
2. `Cargo.lock` no está regenerado: `--locked` fallará hasta hacerlo.
3. `Position` se lee al pedirse; no se emite `Seeked`.

## Diagnóstico gapless
`MUZORYN_DEBUG_GAPLESS=1` registra en `/tmp/muzoryn-gapless.log` cada intento de encadenado.
Antes de probar: `muzoryn ctl quit` (un daemon viejo sigue con el binario antiguo) y arrancar de nuevo con la variable.
