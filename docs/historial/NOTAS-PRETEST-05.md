# MUZORYN 7.0.1 — Pretest 05 (correcciones de seguridad 2 y 3)

Parte del Pretest 04. Versión declarada: 7.0.1 (Cargo.toml, Cargo.lock, User-Agent `muzoryn/7.0.1`).

## Ítem 2 — Límite de tiempo en descargas (`src/feeds.rs`)
Nuevo `DownloadLimits` (valores por defecto):
- conexión: 15 s · silencio del servidor (sin datos): 30 s
- duración total por episodio: 2 h
- velocidad mínima: 16 KiB por cada ventana de 60 s (corta servidores que «gotean» bytes)
- Al cortar se borra el `.part` y el mensaje indica el motivo («Descarga cortada: …»).
- `download_episode_with_limits` permite pasar otros límites; la app usa los de por defecto.

## Ítem 3 — Colisión de nombres
- Dos episodios distintos con el mismo título y extensión ya no se pisan: el segundo se guarda como `Título [hash].ext` (hash FNV-1a estable de la URL).
- Cada carpeta de canal guarda `.muzoryn-index.tsv` (archivo → URL) para que cada episodio encuentre siempre su archivo.
- La reserva del nombre final se hace bajo candado, así que dos descargas simultáneas con el mismo título también quedan a salvo.
- `pending_downloads_for_podcast` deduplica por URL (antes por nombre: el canal completo (`D`) omitía el segundo episodio con título repetido).

## Limitación conocida
Una descarga antigua sin entrada en el índice se considera del episodio que la pide (se evita duplicar lo ya descargado, pero un homónimo antiguo puede figurar como «descargado»).

## Pruebas (+8, total 154)
Servidor mudo, goteo lento, tiempo total, descarga sana, colisión secuencial, colisión concurrente, deduplicación por URL, nombre estable. Con servidores TCP locales; repetidas 5 veces sin fallos.
