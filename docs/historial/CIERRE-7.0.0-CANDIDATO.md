# MUZORYN 7.0.0 — candidato para validación final

## Cambios de cierre
- Versión del paquete actualizada a 7.0.0 en Cargo.toml y Cargo.lock.
- En Linux, el proceso principal establece su nombre de proceso a `muzoryn`.
- Cuando se inicia con `--daemon`, el proceso establece su nombre a `muzoryn-daemon`.
- Se utiliza `PR_SET_NAME` mediante libc; ambos nombres caben en el límite de 15 bytes de Linux.
- No se modificaron las rutas funcionales de radio, bibliotecas, podcasts ni reproducción.

## Validación pendiente
Este paquete se entrega como código fuente para validación en Linux. En el entorno de preparación no están disponibles `cargo` ni `rustc`; por tanto, no se afirma que haya compilado ni que las pruebas se hayan ejecutado.

En Linux, desde esta carpeta ejecutar:

```sh
rustc --version
cargo fmt --check
cargo test --locked
cargo build --locked
cargo build --release --locked

# Tras instalar/ejecutar el programa, comprobar los nombres reales:
pgrep -a muzoryn
ps -o pid,comm,args -C muzoryn -C muzoryn-daemon
cat /proc/$(pgrep -n -x muzoryn)/comm
```

La auditoría de seguridad no se considera cerrada solo por actualizar dependencias: requiere ejecutar una herramienta de auditoría de crates (por ejemplo `cargo audit`) en un entorno con acceso al índice y revisar sus avisos.
