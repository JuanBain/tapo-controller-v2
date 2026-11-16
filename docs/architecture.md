# Arquitectura — Tapo Controller (fork RGBIC Desktop)

Estado: auditado en T1 sobre `upstream/master` `46b5ee1` (Tauri v2 + Rust). Este documento describe el código real del checkout, no la hoja de ruta.

## Vista general

```
React (src/App.tsx)
   │  invoke('<comando>', args)            Tauri IPC (JSON)
   ▼
commands.rs  ── frontera IPC: validación (validation.rs), resolución de destino
   │
   ├── credentials.rs  sesión en memoria + Windows Credential Manager (keyring)
   ├── storage.rs      JSON locales en %APPDATA%\com.bartag.tapo-controller\data
   ├── device.rs       TapoDevice / DeviceManager → crate `tapo` (HTTP local, KLAP/passthrough)
   ├── scheduler.rs    tokio-cron-scheduler → device.rs
   └── discovery.rs    escaneo TCP :80 de la /24 local
lib.rs  arranque: estado gestionado, migración/restauración de credenciales, bandeja, scheduler, reconexión
```

La comunicación con las tiras es **local por Wi-Fi** mediante el crate `tapo`. No existe backend cloud propio.

## Módulos

| Módulo | Responsabilidad |
|---|---|
| `src/App.tsx` | Única vista React: login, alta/escaneo de dispositivos, lista, controles de encendido, brillo, color y temperatura, ajustes (autostart). |
| `src/components/TitleBar.tsx` | Barra de título personalizada (ventana sin decoraciones). |
| `src-tauri/src/lib.rs` | Construcción de la app, plugins, estado, bandeja del sistema, cierre a bandeja, registro de comandos. |
| `commands.rs` | Comandos `#[tauri::command]`. Valida la entrada y delega. |
| `validation.rs` | Rangos y formatos de la entrada IPC (alineados con `tapo` 0.8.8). |
| `credentials.rs` | Dueño único del secreto: `SecretStore` (keyring), `Session`, `CredentialManager`, migración heredada. |
| `storage.rs` | Persistencia JSON atómica: presets, horarios, dispositivos, ajustes, `account.json`. Lectura de solo lectura de formatos heredados. |
| `device.rs` | `TapoDevice` (wrapper de `RgbicLightStripHandler`), `DeviceManager`, `resolve_target`, `apply_preset`, redacción de errores. |
| `scheduler.rs` | Jobs cron a partir de `Schedule`; ejecuta acciones sobre un destino resuelto. |
| `discovery.rs` | Lista IPs con el puerto 80 abierto en la /24 local (no identifica el modelo). |

## Dependencias principales (versiones efectivas en los lockfiles)

| Paquete | Versión | Uso |
|---|---|---|
| `tauri` | 2.9.5 | Shell de escritorio, IPC, bandeja |
| `tapo` | 0.8.8 | Protocolo local de Tapo |
| `keyring` | 3.6.3 (`windows-native`) | Windows Credential Manager |
| `tokio` | 1.49.0 | Runtime async |
| `tokio-cron-scheduler` | 0.13.0 | Horarios |
| `tauri-plugin-store` | 2.4.2 | Inicializado; ya no se usa (ver security.md) |
| `tauri-plugin-shell` | 2.3.4 | Inicializado; no se usa |
| `tauri-plugin-autostart` | 2.5.1 | Inicio con Windows |
| `react` / `react-dom` | 19.2.4 | UI |
| `vite` | 6.4.1 | Bundler |
| `@tauri-apps/api` / `cli` | 2.9.1 / 2.9.6 | IPC / build |
| `react-colorful` | 5.6.1 | Selector HSV (inyecta `<style>` en runtime) |
| `tailwindcss` | 3.4.19 | Estilos |

## Flujo de comunicación con dispositivos

1. **Login** (`login`): valida email/contraseña e inicia la sesión en `CredentialManager`. Si Remember Me está activo, la contraseña se guarda en el keyring. Sin IP de dispositivo **no se verifica** la contraseña: no hay dispositivo contra el que autenticar.
2. **Alta** (`add_device`): `TapoDevice::connect(ip)`, que hace un único login `ApiClient::l930(ip)` con un timeout de 10 s y después `get_device_info()`. Solo si tiene éxito se inserta en `DeviceManager` y se persiste en `devices.json`.
3. **Control** (`device_power`, `device_brightness`, `device_color`, `device_temperature`, `device_effect`): validación → `get_target(ip)` → clon del `Arc<TapoDevice>` → **se libera el lock** → petición de red.
4. **Arranque**: `restore()` síncrono (migración y sesión) → scheduler → `auto_reconnect` secuencial de `devices.json` con la sesión restaurada.
5. **Reconexión manual** (`reconnect_device`): nuevo `TapoDevice` con la sesión actual, que sustituye al anterior en el mapa.

### Selección de destino
`resolve_target(map, ip)`: con IP explícita usa esa o devuelve error. Sin IP solo se acepta si hay **exactamente un** dispositivo conectado. Con varios, devuelve error. La bandeja y los horarios sin `device_ip` siguen esta regla.

## Decisiones arquitectónicas (T1)

| Decisión | Motivo | Alternativa descartada |
|---|---|---|
| `CredentialManager` como dueño único del secreto, con `SecretStore` inyectable | Una sola ruta para guardar, leer y borrar, y pruebas sin tocar el Credential Manager real | Mantener `storage.rs` y `plugin-store` en paralelo |
| Mantener servicio/clave de keyring `tapo-controller` / `tapo-password` | Compatibilidad con entradas creadas por upstream | Clave por email (rompería entradas existentes) |
| `account.json` (no secreto) en lugar de `plugin-store` | Testable sin `AppHandle`; el frontend no necesita acceso al store | Seguir usando `app.store()` |
| `HashMap<String, Arc<TapoDevice>>` | No retener el `RwLock` durante la E/S de red | Mutex por dispositivo (más invasivo) |
| Un solo login por conexión | `l930/l920/l900` hacen el mismo login sin detectar el modelo; la cadena triplicaba intentos y espera | Detección de modelo (pendiente, T2) |
| Escritura JSON atómica (tmp + rename) y respaldo de JSON corrupto | Evita archivos truncados y la pérdida silenciosa de datos | Base de datos embebida (cambio de stack) |
| Nuevo comando `add_device` | El webview ya no necesita conservar la contraseña tras el login | Reutilizar `login` con contraseña vacía |

Compatibilidad upstream: se conservan todos los comandos IPC, los formatos de `presets.json`, `schedules.json` (campo `device_ip` opcional), `devices.json` y `settings.json`, el identificador `com.bartag.tapo-controller` y la entrada de keyring.

## Funcionalidad RGBIC existente (sin modificar en T1)

- `device_effect` y `available_effects` exponen 17 presets de `LightingEffectPreset` (verificado por código).
- **React no consume estos comandos**: no hay UI de efectos.
- No hay control por segmentos ni efectos personalizados.
- Soporte físico en L930-5: **no verificado**. Queda para T2.
