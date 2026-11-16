# Seguridad — credenciales, persistencia y configuración Tauri

## Gestión de credenciales (tras T1)

| Dato | Dónde | Cuándo |
|---|---|---|
| Contraseña Tapo | Windows Credential Manager, entrada genérica `tapo-password` del servicio `tapo-controller` | Solo con **Remember me** activo |
| Contraseña (sesión) | Memoria del proceso (`CredentialManager` y cada `TapoDevice`) | Mientras la app está abierta |
| Email y `remember_me` | `%APPDATA%\com.bartag.tapo-controller\data\account.json` | Tras el login |
| Presets, horarios, dispositivos, ajustes | `...\data\*.json` | Sin secretos |

Reglas de la implementación:
- Ningún archivo JSON nuevo contiene la contraseña.
- `get_saved_credentials` devuelve siempre `password: ""`. El webview no recibe nunca la contraseña guardada, y la UI la borra de su estado tras el login.
- `Session` y `LegacyCredentials` implementan `Debug` con `<redacted>`.
- Los errores de dispositivo pasan por `redact()`, que elimina el email y la contraseña antes de llegar a la UI o a los logs.
- Los logs registran el estado de la migración (`PasswordMigrated`, `Conflict`, …) y nunca valores.

### Remember me
- **Activo**: contraseña en keyring. Al reiniciar se restaura la sesión y se reconectan los dispositivos.
- **Inactivo**: la contraseña solo existe en memoria. Se borra la entrada del keyring si existía. Al reiniciar hay que iniciar sesión otra vez.

### Cierre de sesión (`logout`)
Vacía los dispositivos conectados, la sesión en memoria, la entrada del keyring, `account.json` (queda vacío como marca de migración hecha) y **el archivo heredado `data/credentials.json`**, como ya hacía upstream. `clear_credentials_cmd` borra lo recordado sin cerrar la sesión.

## Migración desde el almacenamiento anterior

Upstream (≤ 2.0.0) escribía:
1. `data/credentials.json` con `{email, password, device_ip}` **en texto plano**, en cada login y con independencia de Remember me. Era la fuente real de la sesión y de la reconexión.
2. `credentials.json` (`tauri-plugin-store`) con `{email, rememberMe}`, más la contraseña en el keyring si había Remember me.

La migración se ejecuta una vez al arrancar si no existe `account.json`:

| Situación | Acción | Resultado |
|---|---|---|
| Sin archivos heredados | Nada | `NothingToMigrate` |
| Solo el archivo de plugin-store | Copia email/rememberMe a `account.json` | `PreferencesOnly` |
| Texto plano y `rememberMe=false` | `account.json` con remember=false; **no** se guarda la contraseña | `NotRemembered` |
| Texto plano y keyring vacío (rememberMe true o ausente) | Escribe en el keyring, **relee y compara**, y después `account.json` | `PasswordMigrated` |
| Texto plano y keyring con el mismo valor | Solo `account.json` | `PasswordMigrated` |
| Texto plano y keyring con **otro** valor | No sobrescribe nada; sesión en memoria con el valor heredado; se reintenta en el próximo arranque | `Conflict` |
| Fallo del keyring o relectura distinta | No escribe `account.json`; sesión en memoria; se reintenta | `Failed` |

Garantías:
- Nunca se sobrescribe ni se borra una entrada del keyring.
- Los archivos heredados solo se leen. Un JSON heredado corrupto se deja intacto.
- El archivo en texto plano se elimina **solo en un arranque posterior** si se cumplen todas estas condiciones: la migración ya se había guardado antes (`AlreadyMigrated`), la contraseña se lee dos veces del Credential Manager y ambas lecturas coinciden, esa contraseña es idéntica a la del archivo, y **todos** los dispositivos guardados se reconectan. Si falta cualquiera, el archivo se conserva y el motivo queda en el log.
- El arranque que hace la migración nunca borra el archivo, aunque la escritura en el keyring se haya releído bien.
- Con Remember me desactivado no hay contraseña en el Credential Manager, así que la condición falla y el archivo se conserva.
- Cerrar sesión sigue eliminando el archivo, porque es una acción explícita de borrar las credenciales.
- Un `Conflict` se resuelve iniciando sesión de nuevo con Remember me, lo que escribe `account.json`.

## Riesgos encontrados y correcciones

| # | Riesgo | Gravedad | Estado |
|---|---|---|---|
| S1 | Contraseña en texto plano en `data/credentials.json`, siempre y sin respetar Remember me | Crítica | **Corregido** para escrituras nuevas. El archivo existente se borra solo tras un reinicio verificado (keyring + reconexión de todos los dispositivos) o al cerrar sesión. En este equipo ya no existe (verificado el 16 nov 2026, ver `t1-report.md` §8) |
| S2 | `get_saved_credentials` enviaba la contraseña del keyring al webview | Alta | **Corregido** |
| S3 | La ventana tenía `store:default`: un script en el webview podía leer cualquier store del directorio de datos, incluido el texto plano | Alta | **Corregido** (permiso retirado; el frontend no lo usa) |
| S4 | `shell:allow-open` concedido sin uso | Media | **Corregido** (permiso retirado) |
| S5 | `csp: null` | Media | **Corregido**: CSP restrictiva en producción |
| S6 | Remember me ignorado: la sesión persistía siempre | Media | **Corregido** |
| S7 | Los mensajes de error podían reenviar texto de librerías sin filtrar | Baja | **Mitigado** con `redact()` |
| S8 | Login triple por conexión (riesgo de bloqueo temporal del dispositivo con contraseña errónea) | Baja | **Corregido** (un intento) |
| S9 | NSIS `installMode: perMachine` exige permisos de administrador | Info | Sin cambio |

### CSP aplicada
```
default-src 'self' ipc: http://ipc.localhost; script-src 'self'; style-src 'self' 'unsafe-inline';
img-src 'self' data:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'
```
- Tauri 2.9.5 la aplica como cabecera a los assets empaquetados (build). En `npm run dev` el contenido lo sirve Vite y no se aplica.
- `style-src 'unsafe-inline'` es necesario porque `react-colorful` inyecta un `<style>` en runtime. Tauri solo añade nonces a `style-src` si el HTML contiene estilos inline, y el `index.html` generado no tiene ninguno.
- **Pendiente de validación visual** en el build instalado (checklist en `t1-report.md`).

### Capabilities resultantes (`capabilities/default.json`)
`core:default`, permisos de ventana (cerrar, minimizar, maximizar, ocultar, mostrar, foco, arrastrar) y `autostart:default`.

## Limitaciones y pendientes

1. **App upstream instalada con inicio automático** (`C:\Program Files\Tapo Controller`, clave `Run` de HKCU). Comparte `%APPDATA%\com.bartag.tapo-controller` y, si se inicia sesión en ella, vuelve a crear `data\credentials.json` en texto plano. Acción manual: desactivar su inicio automático o desinstalarla. El fork migraría de nuevo el archivo de forma segura, pero se reabriría la exposición en disco.
2. La contraseña vive en memoria en `String` sin borrado seguro (`zeroize`). Añadirlo implica una dependencia nueva; no se hizo.
3. El keyring guarda una única cuenta por usuario de Windows (mismo diseño que upstream).
4. `tauri-plugin-store` y `tauri-plugin-shell` siguen inicializados en Rust sin uso. Quitarlos cambia `Cargo.toml`, `Cargo.lock` y `package.json`; se propone para una tarea aparte.
5. `npm audit` reporta 10 vulnerabilidades (1 baja, 2 moderadas, 7 altas), **todas en dependencias de desarrollo/build**: `vite` ≤ 6.4.3, `rollup`, `postcss`, `@babel/core`, `nanoid`, `picomatch`, etc. `npm audit --omit=dev` da 0. No se ejecutó `npm audit fix` para no actualizar en masa; propuesta: subir `vite` dentro de la rama 6.x en una tarea revisada.
7. El login sin dispositivo no verifica la contraseña (limitación del protocolo local).
