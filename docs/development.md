# Desarrollo en Windows 11

## Requisitos (versiones verificadas en T1)

| Herramienta | Versión usada | Notas |
|---|---|---|
| Windows | 11 (10.0.26300) | |
| Node.js | 22.19.0 | CI usa Node 20 |
| npm | 10.9.3 | |
| Rust (rustup, stable MSVC) | rustc / cargo 1.100.0 | `stable-x86_64-pc-windows-msvc` |
| Visual Studio Build Tools | 2026 (componente VC x86/x64) | Necesario para el linker MSVC |
| WebView2 Runtime | 157.0.4341.46 | Incluido en Windows 11 |
| Git | 2.55 | |

Si `cargo` no se reconoce tras instalar Rust, abre una terminal nueva o añade `%USERPROFILE%\.cargo\bin` al `PATH`.

## Instalación

```bash
npm ci                 # usa package-lock.json, sin actualizar dependencias
```
Las dependencias Rust se descargan en la primera compilación según `src-tauri/Cargo.lock`.

## Ejecución local

```bash
npm run dev            # tauri dev: arranca Vite (puerto 5173) y la app de escritorio
npm run dev:vite       # solo el frontend en el navegador (sin IPC de Tauri)
```

> El primer arranque de esta versión migra las credenciales del formato upstream (ver `security.md`). Usa el mismo directorio de datos que la app instalada: `%APPDATA%\com.bartag.tapo-controller`.

## Compilación

```bash
npm run build:frontend # vite build → dist/
npm run build          # tauri build → release + instaladores
```
Artefactos:
- `src-tauri/target/release/tapo-controller.exe`
- `src-tauri/target/release/bundle/msi/Tapo Controller_2.0.0_x64_en-US.msi`
- `src-tauri/target/release/bundle/nsis/Tapo Controller_2.0.0_x64-setup.exe`

En la primera ejecución, la CLI de Tauri descarga WiX 3.14 y NSIS 3.11 a su caché local.

## Pruebas

```bash
npx tsc --noEmit -p .        # chequeo de tipos (vite build no lo hace)
cd src-tauri
cargo check
cargo clippy
cargo test                   # 46 pruebas unitarias, sin hardware
```

- Las pruebas no usan el Credential Manager real: `credentials.rs` usa un `SecretStore` en memoria.
- Las pruebas de `storage` usan directorios temporales y los borran al terminar.
- Los secretos de prueba son cadenas ficticias (`dummy-test-secret`).
- No hay framework de pruebas de frontend; añadirlo (p. ej. Vitest) requiere aprobación de dependencia.
- Pruebas con hardware: checklist manual en `t1-report.md`.

## Ramas

`master` (estable, sigue a upstream) → `develop` (integración) → `feature/<tarea>-<descripcion>`. Sin worktrees. Commits, push y PR solo con aprobación.
