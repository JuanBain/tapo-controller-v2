"""Tapo device wrapper for LED strip control."""
from tapo import ApiClient
from tapo.requests import Color, LightingEffectPreset
import asyncio
from typing import Any


class TapoDevice:
    """Wrapper for Tapo LED strip devices."""

    def __init__(self, email: str, password: str):
        self.email = email
        self.password = password
        self.client: ApiClient | None = None
        self.device: Any = None
        self.device_ip: str | None = None

    async def connect(self, ip: str) -> dict:
        """Connect to a Tapo device by IP."""
        self.client = ApiClient(self.email, self.password)
        # Try L930 first (most features), fallback to L920, then L900
        try:
            self.device = await self.client.l930(ip)
        except Exception:
            try:
                self.device = await self.client.l920(ip)
            except Exception:
                self.device = await self.client.l900(ip)

        self.device_ip = ip
        info = await self.device.get_device_info()
        return self._parse_device_info(info)

    def _parse_device_info(self, info: Any) -> dict:
        """Parse device info to a clean dict."""
        return {
            "device_id": getattr(info, "device_id", "unknown"),
            "nickname": getattr(info, "nickname", "LED Strip"),
            "model": getattr(info, "model", "unknown"),
            "device_on": getattr(info, "device_on", False),
            "brightness": getattr(info, "brightness", 100),
            "hue": getattr(info, "hue", None),
            "saturation": getattr(info, "saturation", None),
            "color_temp": getattr(info, "color_temp", None),
            "ip": self.device_ip,
        }

    async def get_info(self) -> dict:
        """Get current device state."""
        if not self.device:
            raise RuntimeError("Not connected to device")
        info = await self.device.get_device_info()
        return self._parse_device_info(info)

    async def turn_on(self) -> None:
        """Turn device on."""
        if not self.device:
            raise RuntimeError("Not connected to device")
        await self.device.on()

    async def turn_off(self) -> None:
        """Turn device off."""
        if not self.device:
            raise RuntimeError("Not connected to device")
        await self.device.off()

    async def set_brightness(self, brightness: int) -> None:
        """Set brightness (1-100)."""
        if not self.device:
            raise RuntimeError("Not connected to device")
        brightness = max(1, min(100, brightness))
        await self.device.set_brightness(brightness)

    async def set_color(self, hue: int, saturation: int) -> None:
        """Set color via hue (0-360) and saturation (0-100)."""
        if not self.device:
            raise RuntimeError("Not connected to device")
        hue = max(0, min(360, hue))
        saturation = max(0, min(100, saturation))
        await self.device.set_hue_saturation(hue, saturation)

    async def set_color_temperature(self, kelvin: int) -> None:
        """Set color temperature (2500-6500K)."""
        if not self.device:
            raise RuntimeError("Not connected to device")
        kelvin = max(2500, min(6500, kelvin))
        await self.device.set_color_temperature(kelvin)

    async def set_effect(self, effect_name: str) -> None:
        """Set a lighting effect preset (L920/L930 only)."""
        if not self.device:
            raise RuntimeError("Not connected to device")

        # Map effect names to presets
        effect_map = {
            "aurora": LightingEffectPreset.Aurora,
            "bubbling_cauldron": LightingEffectPreset.BubblingCauldron,
            "candycane": LightingEffectPreset.CandyCane,
            "christmas": LightingEffectPreset.Christmas,
            "flicker": LightingEffectPreset.Flicker,
            "grandmas_christmas_lights": LightingEffectPreset.GrandmasChristmasLights,
            "hanukkah": LightingEffectPreset.Hanukkah,
            "haunted_mansion": LightingEffectPreset.HauntedMansion,
            "icicle": LightingEffectPreset.Icicle,
            "lightning": LightingEffectPreset.Lightning,
            "ocean": LightingEffectPreset.Ocean,
            "rainbow": LightingEffectPreset.Rainbow,
            "raindrop": LightingEffectPreset.Raindrop,
            "spring": LightingEffectPreset.Spring,
            "valentines": LightingEffectPreset.Valentines,
        }

        effect = effect_map.get(effect_name.lower())
        if effect:
            await self.device.set_lighting_effect(effect)
        else:
            raise ValueError(f"Unknown effect: {effect_name}")

    @staticmethod
    def get_available_effects() -> list[str]:
        """Get list of available effect names."""
        return [
            "aurora",
            "bubbling_cauldron",
            "candycane",
            "christmas",
            "flicker",
            "grandmas_christmas_lights",
            "hanukkah",
            "haunted_mansion",
            "icicle",
            "lightning",
            "ocean",
            "rainbow",
            "raindrop",
            "spring",
            "valentines",
        ]


# Global device instance
_device: TapoDevice | None = None


def get_device() -> TapoDevice | None:
    return _device


def set_device(device: TapoDevice) -> None:
    global _device
    _device = device
