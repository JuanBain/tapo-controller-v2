"""Schedule management with APScheduler."""
from apscheduler.schedulers.asyncio import AsyncIOScheduler
from apscheduler.triggers.cron import CronTrigger
from datetime import datetime
import asyncio
from typing import Callable, Awaitable

from storage import get_schedules, get_preset, Schedule
from device import get_device


scheduler = AsyncIOScheduler()
_schedule_jobs: dict[str, str] = {}  # schedule_id -> job_id


async def execute_schedule(schedule: Schedule) -> None:
    """Execute a schedule action."""
    device = get_device()
    if not device or not device.device:
        print(f"Cannot execute schedule {schedule.name}: No device connected")
        return

    try:
        if schedule.action == "on":
            await device.turn_on()
            if schedule.brightness:
                await device.set_brightness(schedule.brightness)
        elif schedule.action == "off":
            await device.turn_off()
        elif schedule.action == "preset" and schedule.preset_id:
            preset = get_preset(schedule.preset_id)
            if preset:
                await device.turn_on()
                await device.set_brightness(preset.brightness)
                if preset.hue is not None and preset.saturation is not None:
                    await device.set_color(preset.hue, preset.saturation)
                elif preset.color_temperature:
                    await device.set_color_temperature(preset.color_temperature)
                if preset.effect:
                    await device.set_effect(preset.effect)
        print(f"Executed schedule: {schedule.name}")
    except Exception as e:
        print(f"Failed to execute schedule {schedule.name}: {e}")


def _create_job_func(schedule: Schedule) -> Callable[[], Awaitable[None]]:
    """Create a job function for the schedule."""
    async def job():
        await execute_schedule(schedule)
    return job


def schedule_to_cron(schedule: Schedule) -> CronTrigger:
    """Convert schedule to APScheduler cron trigger."""
    hour, minute = map(int, schedule.time.split(":"))

    # Convert day numbers to cron day_of_week format
    # APScheduler: mon=0, sun=6 (same as our format)
    if schedule.days:
        day_of_week = ",".join(str(d) for d in schedule.days)
    else:
        day_of_week = "*"  # Every day

    return CronTrigger(
        hour=hour,
        minute=minute,
        day_of_week=day_of_week,
    )


def add_schedule_job(schedule: Schedule) -> None:
    """Add a schedule to the scheduler."""
    if not schedule.enabled:
        return

    job_id = f"schedule_{schedule.id}"
    trigger = schedule_to_cron(schedule)

    # Remove existing job if any
    if schedule.id in _schedule_jobs:
        try:
            scheduler.remove_job(_schedule_jobs[schedule.id])
        except Exception:
            pass

    # Add new job
    scheduler.add_job(
        _create_job_func(schedule),
        trigger=trigger,
        id=job_id,
        name=schedule.name,
        replace_existing=True,
    )
    _schedule_jobs[schedule.id] = job_id


def remove_schedule_job(schedule_id: str) -> None:
    """Remove a schedule from the scheduler."""
    if schedule_id in _schedule_jobs:
        try:
            scheduler.remove_job(_schedule_jobs[schedule_id])
        except Exception:
            pass
        del _schedule_jobs[schedule_id]


def reload_all_schedules() -> None:
    """Load all schedules from storage into the scheduler."""
    # Clear existing jobs
    for job_id in list(_schedule_jobs.values()):
        try:
            scheduler.remove_job(job_id)
        except Exception:
            pass
    _schedule_jobs.clear()

    # Load schedules
    schedules = get_schedules()
    for schedule in schedules:
        if schedule.enabled:
            add_schedule_job(schedule)


def start_scheduler() -> None:
    """Start the scheduler."""
    if not scheduler.running:
        reload_all_schedules()
        scheduler.start()


def stop_scheduler() -> None:
    """Stop the scheduler."""
    if scheduler.running:
        scheduler.shutdown()
