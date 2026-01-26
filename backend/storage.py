"""JSON-based persistence for presets and schedules."""
import json
import sys
import os
from pathlib import Path
from typing import Any
from pydantic import BaseModel
from datetime import datetime
import uuid


def get_data_dir() -> Path:
    """Get the data directory, handling PyInstaller frozen state."""
    # Check if running as PyInstaller bundle
    if getattr(sys, 'frozen', False):
        # Running as compiled executable
        if sys.platform == 'win32':
            base = Path(os.environ.get('APPDATA', Path.home()))
        elif sys.platform == 'darwin':
            base = Path.home() / 'Library' / 'Application Support'
        else:
            base = Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local' / 'share'))
        data_dir = base / 'tapo-controller' / 'data'
    else:
        # Running as script (development)
        data_dir = Path(__file__).parent / "data"

    data_dir.mkdir(parents=True, exist_ok=True)
    return data_dir


DATA_DIR = get_data_dir()

CREDENTIALS_FILE = DATA_DIR / "credentials.json"
PRESETS_FILE = DATA_DIR / "presets.json"
SCHEDULES_FILE = DATA_DIR / "schedules.json"
SETTINGS_FILE = DATA_DIR / "settings.json"
DEVICES_FILE = DATA_DIR / "devices.json"


class Preset(BaseModel):
    id: str
    name: str
    hue: int | None = None
    saturation: int | None = None
    brightness: int
    color_temperature: int | None = None
    effect: str | None = None
    created_at: str


class Schedule(BaseModel):
    id: str
    name: str
    time: str  # HH:MM format
    days: list[int]  # 0=Monday, 6=Sunday
    action: str  # "on", "off", "preset"
    preset_id: str | None = None
    brightness: int | None = None
    enabled: bool = True
    created_at: str


class Credentials(BaseModel):
    email: str
    password: str
    device_ip: str | None = None


class Settings(BaseModel):
    last_device_ip: str | None = None


class SavedDevice(BaseModel):
    ip: str
    nickname: str | None = None
    model: str | None = None
    added_at: str | None = None


def _load_json(path: Path) -> dict | list:
    if not path.exists():
        return [] if "presets" in path.name or "schedules" in path.name else {}
    with open(path, "r") as f:
        return json.load(f)


def _save_json(path: Path, data: Any) -> None:
    with open(path, "w") as f:
        json.dump(data, f, indent=2)


# Credentials
def save_credentials(creds: Credentials) -> None:
    _save_json(CREDENTIALS_FILE, creds.model_dump())


def load_credentials() -> Credentials | None:
    data = _load_json(CREDENTIALS_FILE)
    if not data:
        return None
    return Credentials(**data)


def clear_credentials() -> None:
    if CREDENTIALS_FILE.exists():
        CREDENTIALS_FILE.unlink()


# Presets
def get_presets() -> list[Preset]:
    data = _load_json(PRESETS_FILE)
    return [Preset(**p) for p in data]


def save_preset(preset: Preset) -> Preset:
    presets = get_presets()
    preset.id = str(uuid.uuid4())[:8]
    preset.created_at = datetime.now().isoformat()
    presets.append(preset)
    _save_json(PRESETS_FILE, [p.model_dump() for p in presets])
    return preset


def delete_preset(preset_id: str) -> bool:
    presets = get_presets()
    new_presets = [p for p in presets if p.id != preset_id]
    if len(new_presets) == len(presets):
        return False
    _save_json(PRESETS_FILE, [p.model_dump() for p in new_presets])
    return True


def get_preset(preset_id: str) -> Preset | None:
    presets = get_presets()
    for p in presets:
        if p.id == preset_id:
            return p
    return None


# Schedules
def get_schedules() -> list[Schedule]:
    data = _load_json(SCHEDULES_FILE)
    return [Schedule(**s) for s in data]


def save_schedule(schedule: Schedule) -> Schedule:
    schedules = get_schedules()
    schedule.id = str(uuid.uuid4())[:8]
    schedule.created_at = datetime.now().isoformat()
    schedules.append(schedule)
    _save_json(SCHEDULES_FILE, [s.model_dump() for s in schedules])
    return schedule


def update_schedule(schedule_id: str, schedule: Schedule) -> Schedule | None:
    schedules = get_schedules()
    for i, s in enumerate(schedules):
        if s.id == schedule_id:
            schedule.id = schedule_id
            schedule.created_at = s.created_at
            schedules[i] = schedule
            _save_json(SCHEDULES_FILE, [s.model_dump() for s in schedules])
            return schedule
    return None


def delete_schedule(schedule_id: str) -> bool:
    schedules = get_schedules()
    new_schedules = [s for s in schedules if s.id != schedule_id]
    if len(new_schedules) == len(schedules):
        return False
    _save_json(SCHEDULES_FILE, [s.model_dump() for s in new_schedules])
    return True


# Settings
def save_settings(settings: Settings) -> None:
    _save_json(SETTINGS_FILE, settings.model_dump())


def load_settings() -> Settings:
    data = _load_json(SETTINGS_FILE)
    if not data:
        return Settings()
    return Settings(**data)


def save_last_ip(ip: str) -> None:
    settings = load_settings()
    settings.last_device_ip = ip
    save_settings(settings)


def get_last_ip() -> str | None:
    return load_settings().last_device_ip


# Saved Devices
def get_saved_devices() -> list[SavedDevice]:
    data = _load_json(DEVICES_FILE)
    if isinstance(data, list):
        return [SavedDevice(**d) for d in data]
    return []


def save_device_info(device: SavedDevice) -> SavedDevice:
    devices = get_saved_devices()
    # Update if exists, otherwise add
    for i, d in enumerate(devices):
        if d.ip == device.ip:
            devices[i] = device
            _save_json(DEVICES_FILE, [d.model_dump() for d in devices])
            return device
    device.added_at = datetime.now().isoformat()
    devices.append(device)
    _save_json(DEVICES_FILE, [d.model_dump() for d in devices])
    return device


def remove_saved_device(ip: str) -> bool:
    devices = get_saved_devices()
    new_devices = [d for d in devices if d.ip != ip]
    if len(new_devices) == len(devices):
        return False
    _save_json(DEVICES_FILE, [d.model_dump() for d in new_devices])
    return True


def get_saved_device(ip: str) -> SavedDevice | None:
    for d in get_saved_devices():
        if d.ip == ip:
            return d
    return None
