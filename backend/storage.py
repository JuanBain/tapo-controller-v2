"""JSON-based persistence for presets and schedules."""
import json
from pathlib import Path
from typing import Any
from pydantic import BaseModel
from datetime import datetime
import uuid


DATA_DIR = Path(__file__).parent / "data"
DATA_DIR.mkdir(exist_ok=True)

CREDENTIALS_FILE = DATA_DIR / "credentials.json"
PRESETS_FILE = DATA_DIR / "presets.json"
SCHEDULES_FILE = DATA_DIR / "schedules.json"


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
