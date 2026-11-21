# 🔒 Seguridad y privacidad

## Qué datos salen de tu equipo
| Función | Qué se envía | Cómo evitarlo |
|---|---|---|
| Radio Browser | Solo el `stationuuid` de la emisora que reproduces (para contar popularidad), y tus búsquedas | `radio_click_count = false` y no uses Explorar Radio Browser |
| Directorio de podcasts | Tus búsquedas al directorio público de Apple Podcasts (sin cuenta ni clave) | No uses `/` ni `Ctrl+B`; añade feeds por URL |
| Emisoras y feeds por URL | Las peticiones HTTP normales a esos servidores; el `User-Agent` es `muzoryn/<versión>` | — |

MUZORYN no envía estadísticas ni telemetría propias.

## Medidas de seguridad (7.0.1)
| Área | Medida |
|---|---|
| **Hooks** | Solo con `settings.toml` de confianza (tuyo, sin escritura de grupo/otros, carpeta incluida); cuarentena sin pérdida de datos; variables de entorno en lugar de interpolación; 30 s de límite, 8 simultáneos, 16 máximo ([detalle](hooks.md)) |
| **Redirecciones HTTP** | Se siguen a mano, máx. 5 saltos; se bloquea HTTPS→HTTP; solo `http(s)`; destinos inválidos rechazados. HTTP→HTTP y HTTP→HTTPS siguen permitidos porque muchas radios son HTTP. Aplica a todos los clientes |
| **Instalador** | Sin `curl \| sh`: confirmación explícita, descarga HTTPS (TLS 1.2) y verificación SHA-256 de `rustup-init` |
| **Descargas** | Límites de conexión, silencio, duración total y velocidad mínima; borrado de parciales; nombres sin colisión (`[hash]` basado en la URL) |
| **Archivos y socket** | Carpeta de configuración `0700`; `settings.toml` y socket del daemon `0600` |
| **Cierre del daemon** | Ordenado, sin `process::exit`; borra el socket |
| **Importadores** | Las listas de emisoras filtran esquemas distintos de `http(s)` |
| **MPRIS** | Solo en el bus de **sesión** de D-Bus (nunca el del sistema); no abre puertos de red. Ver nota abajo |

## MPRIS y privacidad local
MPRIS publica en el bus de sesión el título, artista, álbum y, en pistas por URL, la dirección completa (`xesam:url`). Cualquier programa de tu sesión de usuario puede leerlos y controlar la reproducción; es el comportamiento estándar de MPRIS. Si la URL de un podcast lleva un token personal, otros programas de tu sesión podrían verlo. Para no exponer nada, compila sin MPRIS: `cargo build --release --no-default-features` ([MPRIS](mpris.md)).

## Lo que no está verificado
- Registro real de MPRIS en el bus D-Bus del escritorio y control con `playerctl` (pendiente de prueba manual).
- Continuidad audible del *gapless* ([Gapless](gapless.md)).
- Instalación en Arch Linux con descarga real de rustup (se probó con un `curl` simulado).
- Redirecciones HTTPS→HTTP contra servidores reales (la política se prueba por unidad y con servidores HTTP locales).
- Audio real y metadatos ICY contra emisoras reales.

## Auditoría de dependencias
Actualizar dependencias no equivale a una auditoría. Para revisarlas, ejecuta una herramienta de auditoría de crates (por ejemplo `cargo audit`) con acceso al índice y revisa sus avisos.

## Informar de un problema de seguridad
Sigue las instrucciones de [SECURITY.md](../SECURITY.md): evita publicar detalles explotables en un *issue* abierto.
