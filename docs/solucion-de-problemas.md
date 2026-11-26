# 🆘 Solución de problemas

## `muzoryn: command not found`
Añade la carpeta de instalación al `PATH` (en `~/.bashrc` o `~/.zshrc`) y reabre la terminal:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

## Error al compilar: `alsa` / `pkg-config` no encontrado
Falta la librería de audio de desarrollo. Instala la de tu sistema ([Instalación](instalacion.md#opción-b--dependencias-por-distribución)); en Debian/Ubuntu es `libasound2-dev`.

## `rustc` demasiado antiguo
Necesitas Rust ≥ 1.82. Instálalo desde <https://rustup.rs> y ejecuta `./install.sh --no-deps`, o deja que el instalador lo haga con descarga verificada.

## El instalador dice «no hay terminal para confirmar»
Estás ejecutándolo sin terminal interactiva y Rust no está disponible. Repite con `./install.sh --yes` o instala Rust antes.

## No suena nada
- Comprueba que el volumen del sistema no está en silencio y que otra app reproduce audio.
- Sube el volumen interno con `=`.
- En PipeWire/PulseAudio asegúrate de tener la capa de compatibilidad ALSA.
- Cierra todo con `W` o `muzoryn ctl quit` y vuelve a abrir.

## Veo cuadritos ▯ en lugar de iconos
Instala una [Nerd Font](https://www.nerdfonts.com/font-downloads) y selecciónala en tu terminal.

## No salen las carátulas / salen borrosas
Tu terminal quizá no soporta gráficos, o estás dentro de **tmux** (usa bloques de baja resolución). Fuerza un modo con `MUZORYN_GRAPHICS=kitty|iterm|sixel|blocks`.

## Mi biblioteca está vacía
Pulsa `Alt+M` y comprueba que tu carpeta está añadida (o revisa `music_dirs`), luego `u` para escanear. Formatos: mp3, flac, ogg/opus, wav, m4a/aac, aiff.

## Una emisora no conecta
Se esperan hasta ~8 s para conectar. Pulsa `s` para cancelar y elige otra. Algunos formatos de flujo pueden no estar soportados por el decodificador (Symphonia). Si el problema persiste, abre un issue con la URL de la emisora.

## Mis hooks no se ejecutan
Mira **Ajustes → Avanzado** o ejecuta `muzoryn hooks`: si aparecen «en cuarentena», `settings.toml` o su carpeta son escribibles por otros. Corrige los permisos y ejecuta `muzoryn hooks restore --yes` ([Hooks](hooks.md)).

## Una descarga falla o se corta
Mira el mensaje: «Descarga fallida: el servidor respondió HTTP …» indica que el servidor rechazó la petición (se reintenta hasta 3 veces). «Descarga cortada: …» indica que se superó un límite de tiempo o velocidad. Repetir `D` añade solo los episodios pendientes ([Descargas](descargas.md)).

## La interfaz muestra la cola vacía tras actualizar
Interfaz y daemon son de versiones distintas. Ejecuta `muzoryn ctl quit` y abre `muzoryn` otra vez.

## Error de configuración al abrir
Un `settings.toml` o `keys.toml` con sintaxis inválida se informa antes de abrir la interfaz. Renómbralo (`mv settings.toml settings.toml.bak`) y MUZORYN creará uno nuevo.

## ¿Sigue sin funcionar?
Abre un *issue* con: tu distribución, tu terminal, la versión (`muzoryn --version`) y los pasos para reproducirlo.
