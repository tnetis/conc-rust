# Conc (Rust)

Port en **Rust** de la app de escritorio "Conc" (originalmente en Python/Flet),
que envuelve **FFmpeg** para editar vídeo/audio de forma sencilla.

Interfaz con `egui`/`eframe`, tema oscuro con acento ámbar, 5 pestañas:

- **Combine** — junta una imagen o vídeo con un audio y produce un MP4 1920x1080.
- **Compress** — comprime en lote todos los vídeos de una carpeta (H.264, CRF/Preset configurables) y genera un reporte.
- **Cut** — corta un vídeo con tiempos de inicio/fin.
- **Multi-Cut** — corta/rota varios vídeos a la vez leyendo instrucciones `ID INICIO - FIN [ROT]`.
- **Rename** — renombra archivos de una carpeta (aleatorio o buscar/reemplazar).

## Requisitos

- **FFmpeg** y **ffprobe** instalados y en el PATH (o selecciona la ruta del ejecutable en *Ajustes*).
  - Windows: https://www.gyan.dev/ffmpeg/builds/
  - Linux: `sudo apt install ffmpeg`

## Compilar y ejecutar localmente

Necesitas Rust (https://rustup.rs). En Linux, además del toolchain, instala las dependencias de GTK3:

```bash
sudo apt install -y libgtk-3-dev libxkbcommon-dev libxcb1-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libx11-dev libwayland-dev pkg-config
```

```bash
cargo build --release      # compila
cargo run --release        # ejecuta
cargo test                 # tests unitarios
```

El binario queda en `target/release/conc` (Linux) o `target/release/conc.exe` (Windows).

## Compilación automática (GitHub Actions)

Al subir este repositorio a GitHub, el workflow `.github/workflows/build.yml` compila
automáticamente en cada `push` y `pull_request` para:

| SO | runner | binario |
|----|--------|---------|
| Linux (glibc, sirve para cualquier distro de escritorio) | `ubuntu-latest` | `conc` |
| Windows 10/11 (x86_64) | `windows-latest` | `conc.exe` |

Los binarios se publican como *artifacts* de cada ejecución.

### Crear un release

Crea un tag que empiece por `v` (ej. `v0.1.0`) y haz push del tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

El job `release` empaqueta los binarios de ambas plataformas (`.tar.gz`) y los publica
en la página de *Releases* del repositorio automáticamente.

## Estructura

```
src/
├── main.rs       # entry point, UI (egui), tema, pestañas y handlers
├── settings.rs   # ajustes (JSON) con validación
├── time.rs       # parseo/formateo de tiempos
├── ffmpeg.rs     # constructores de comandos, duración (ffprobe), progreso
├── multicut.rs   # parseo de instrucciones multi-cut y emparejamiento
└── jobs.rs       # ejecución en hilos + canal de eventos + operaciones
```

## Notas

- La app invoca `ffmpeg`/`ffprobe` como subprocesos externos (igual que la versión Python).
- Los ajustes se guardan en el directorio de configuración del usuario (`settings.json`).
- En Windows el proceso de FFmpeg se lanza sin ventana de consola (`CREATE_NO_WINDOW`).


## Icono

- **Ventana (Linux y Windows):** se aplica en runtime al abrir la app (`assets/icon.png`, cargado con la crate `image`).
- **Ejecutable de Windows (.exe):** el icono se embebe en el binario con `build.rs` + `assets/icon.ico` (se ve en el Explorador, accesos directos y barra de tareas).
- **Menu de aplicaciones de Linux:** los binarios de Linux no llevan icono embebido. Para que aparezca en el menu, instala el `.desktop` y el PNG:

```bash
# para tu usuario (sin sudo)
install -Dm755 target/release/conc ~/.local/bin/conc
install -Dm644 assets/icon.png ~/.local/share/icons/hicolor/256x256/apps/conc.png
install -Dm644 assets/conc.desktop ~/.local/share/applications/conc.desktop
update-desktop-database ~/.local/share/applications 2>/dev/null || true
```