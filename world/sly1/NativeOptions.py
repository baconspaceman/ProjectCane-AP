"""Additive native-client preferences; never alter the original item pool or rules.

These options request initial settings for first-session review. A client must still
negotiate support and obtain that review before activating any link.
"""
from Options import Toggle


class DeathLink(Toggle):
    """Request shared deaths in the native client's first-session review. Off by default."""
    display_name = "DeathLink"


class RingLink(Toggle):
    """Request shared coin gains in the native client's first-session review. Off by default."""
    display_name = "RingLink"


class EnergyLink(Toggle):
    """Request shared energy exchanges for lives/charms, manual by default. Off by default."""
    display_name = "EnergyLink"


class DamageLink(Toggle):
    """Request shared damage, charms-only by default. Off by default."""
    display_name = "DamageLink"


class TrapLink(Toggle):
    """Request compatible linked traps with seed filters and bounded queue. Off by default."""
    display_name = "TrapLink"


LINK_FIELDS = ("DeathLink", "RingLink", "EnergyLink", "DamageLink", "TrapLink")
BRIDGE_VERSION = 1
BRIDGE_CONTRACT = "sly1-native-v1"
NATIVE_WORLD_VERSION = "0.1.0-dev"


def native_slot_data(options):
    return {
        "bridge_version": BRIDGE_VERSION,
        "bridge_contract": BRIDGE_CONTRACT,
        "native_world_version": NATIVE_WORLD_VERSION,
        **{name: getattr(options, name).value for name in LINK_FIELDS},
    }
