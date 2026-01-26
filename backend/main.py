"""FastAPI backend for Tapo LED Controller."""
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from contextlib import asynccontextmanager

from device import TapoDevice, get_device, set_device
from storage import (
    Credentials, Preset, Schedule,
    save_credentials, load_credentials, clear_credentials,
    get_presets, save_preset, delete_preset, get_preset,
    get_schedules, save_schedule, update_schedule, delete_schedule,
    save_last_ip, get_last_ip,
)
from scheduler import (
    start_scheduler, stop_scheduler, reload_all_schedules,
    add_schedule_job, remove_schedule_job,
)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Startup and shutdown events."""
    # Startup: try to reconnect if we have saved credentials
    creds = load_credentials()
    if creds and creds.device_ip:
        try:
            device = TapoDevice(creds.email, creds.password)
            await device.connect(creds.device_ip)
            set_device(device)
            print(f"Auto-connected to {creds.device_ip}")
        except Exception as e:
            print(f"Auto-connect failed: {e}")

    start_scheduler()
    yield
    stop_scheduler()


app = FastAPI(title="Tapo LED Controller", lifespan=lifespan)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # Electron app
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


# ============ Auth / Connection ============

class LoginRequest(BaseModel):
    email: str
    password: str
    device_ip: str


@app.post("/auth/login")
async def login(req: LoginRequest):
    """Login and connect to device."""
    try:
        device = TapoDevice(req.email, req.password)
        info = await device.connect(req.device_ip)
        set_device(device)

        # Save credentials for auto-reconnect
        save_credentials(Credentials(
            email=req.email,
            password=req.password,
            device_ip=req.device_ip,
        ))

        # Persist IP separately for next login
        save_last_ip(req.device_ip)

        return {"success": True, "device": info}
    except Exception as e:
        raise HTTPException(status_code=401, detail=str(e))


@app.post("/auth/logout")
async def logout():
    """Clear saved credentials."""
    clear_credentials()
    set_device(None)
    return {"success": True}


@app.get("/auth/status")
async def auth_status():
    """Check if connected to device."""
    device = get_device()
    if device and device.device:
        try:
            info = await device.get_info()
            return {"connected": True, "device": info}
        except Exception:
            pass
    return {"connected": False}


@app.get("/auth/last-ip")
async def last_ip():
    """Get last used device IP for pre-filling login form."""
    ip = get_last_ip()
    return {"ip": ip}


# ============ Device Control ============

@app.get("/device/info")
async def device_info():
    """Get device info."""
    device = get_device()
    if not device or not device.device:
        raise HTTPException(status_code=400, detail="Not connected to device")

    try:
        info = await device.get_info()
        return info
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


class PowerRequest(BaseModel):
    on: bool


@app.post("/device/power")
async def device_power(req: PowerRequest):
    """Turn device on/off."""
    device = get_device()
    if not device or not device.device:
        raise HTTPException(status_code=400, detail="Not connected to device")

    try:
        if req.on:
            await device.turn_on()
        else:
            await device.turn_off()
        return {"success": True}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


class BrightnessRequest(BaseModel):
    value: int


@app.post("/device/brightness")
async def device_brightness(req: BrightnessRequest):
    """Set brightness."""
    device = get_device()
    if not device or not device.device:
        raise HTTPException(status_code=400, detail="Not connected to device")

    try:
        await device.set_brightness(req.value)
        return {"success": True}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


class ColorRequest(BaseModel):
    hue: int
    saturation: int


@app.post("/device/color")
async def device_color(req: ColorRequest):
    """Set color."""
    device = get_device()
    if not device or not device.device:
        raise HTTPException(status_code=400, detail="Not connected to device")

    try:
        await device.set_color(req.hue, req.saturation)
        return {"success": True}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


class TemperatureRequest(BaseModel):
    kelvin: int


@app.post("/device/temperature")
async def device_temperature(req: TemperatureRequest):
    """Set color temperature."""
    device = get_device()
    if not device or not device.device:
        raise HTTPException(status_code=400, detail="Not connected to device")

    try:
        await device.set_color_temperature(req.kelvin)
        return {"success": True}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


class EffectRequest(BaseModel):
    name: str


@app.post("/device/effect")
async def device_effect(req: EffectRequest):
    """Set lighting effect."""
    device = get_device()
    if not device or not device.device:
        raise HTTPException(status_code=400, detail="Not connected to device")

    try:
        await device.set_effect(req.name)
        return {"success": True}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/device/effects")
async def available_effects():
    """Get available effects."""
    return {"effects": TapoDevice.get_available_effects()}


# ============ Presets ============

@app.get("/presets")
async def list_presets():
    """List all presets."""
    return {"presets": [p.model_dump() for p in get_presets()]}


class CreatePresetRequest(BaseModel):
    name: str
    hue: int | None = None
    saturation: int | None = None
    brightness: int
    color_temperature: int | None = None
    effect: str | None = None


@app.post("/presets")
async def create_preset(req: CreatePresetRequest):
    """Create a new preset."""
    preset = Preset(
        id="",
        name=req.name,
        hue=req.hue,
        saturation=req.saturation,
        brightness=req.brightness,
        color_temperature=req.color_temperature,
        effect=req.effect,
        created_at="",
    )
    saved = save_preset(preset)
    return {"preset": saved.model_dump()}


@app.delete("/presets/{preset_id}")
async def remove_preset(preset_id: str):
    """Delete a preset."""
    if delete_preset(preset_id):
        return {"success": True}
    raise HTTPException(status_code=404, detail="Preset not found")


@app.post("/presets/{preset_id}/apply")
async def apply_preset(preset_id: str):
    """Apply a preset to the device."""
    device = get_device()
    if not device or not device.device:
        raise HTTPException(status_code=400, detail="Not connected to device")

    preset = get_preset(preset_id)
    if not preset:
        raise HTTPException(status_code=404, detail="Preset not found")

    try:
        await device.turn_on()
        await device.set_brightness(preset.brightness)

        if preset.hue is not None and preset.saturation is not None:
            await device.set_color(preset.hue, preset.saturation)
        elif preset.color_temperature:
            await device.set_color_temperature(preset.color_temperature)

        if preset.effect:
            await device.set_effect(preset.effect)

        return {"success": True}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


# ============ Schedules ============

@app.get("/schedules")
async def list_schedules():
    """List all schedules."""
    return {"schedules": [s.model_dump() for s in get_schedules()]}


class CreateScheduleRequest(BaseModel):
    name: str
    time: str
    days: list[int]
    action: str
    preset_id: str | None = None
    brightness: int | None = None
    enabled: bool = True


@app.post("/schedules")
async def create_schedule(req: CreateScheduleRequest):
    """Create a new schedule."""
    schedule = Schedule(
        id="",
        name=req.name,
        time=req.time,
        days=req.days,
        action=req.action,
        preset_id=req.preset_id,
        brightness=req.brightness,
        enabled=req.enabled,
        created_at="",
    )
    saved = save_schedule(schedule)
    add_schedule_job(saved)
    return {"schedule": saved.model_dump()}


@app.put("/schedules/{schedule_id}")
async def modify_schedule(schedule_id: str, req: CreateScheduleRequest):
    """Update a schedule."""
    schedule = Schedule(
        id=schedule_id,
        name=req.name,
        time=req.time,
        days=req.days,
        action=req.action,
        preset_id=req.preset_id,
        brightness=req.brightness,
        enabled=req.enabled,
        created_at="",
    )
    updated = update_schedule(schedule_id, schedule)
    if not updated:
        raise HTTPException(status_code=404, detail="Schedule not found")

    # Update scheduler
    remove_schedule_job(schedule_id)
    if updated.enabled:
        add_schedule_job(updated)

    return {"schedule": updated.model_dump()}


@app.delete("/schedules/{schedule_id}")
async def remove_schedule(schedule_id: str):
    """Delete a schedule."""
    remove_schedule_job(schedule_id)
    if delete_schedule(schedule_id):
        return {"success": True}
    raise HTTPException(status_code=404, detail="Schedule not found")


@app.post("/schedules/{schedule_id}/toggle")
async def toggle_schedule(schedule_id: str):
    """Toggle schedule enabled state."""
    schedules = get_schedules()
    for s in schedules:
        if s.id == schedule_id:
            s.enabled = not s.enabled
            update_schedule(schedule_id, s)

            if s.enabled:
                add_schedule_job(s)
            else:
                remove_schedule_job(schedule_id)

            return {"enabled": s.enabled}

    raise HTTPException(status_code=404, detail="Schedule not found")


# ============ Health ============

@app.get("/health")
async def health():
    """Health check."""
    return {"status": "ok"}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="127.0.0.1", port=8742)
