from enum import Enum

class Troop(Enum):
    DEMON_HORROR = "demon_horror"
    # Add more troops here later, e.g. BEEFMAN = "beefman"

    @property
    def data(self):
        """All stats for this troop"""
        mapping = {
            "demon_horror": {
                "id": 714,
                "name": "demon_horror",
                "attack_power": 185,      # Red sword (main attack)
                "melee_defence": 19,      # Blue sword
                "ranged_defence": 5,      # Bow
                "loot_capacity": 35,      # Bag icon
                "travel_speed": 28,       # Hourglass (movement / time)
                "food_consumption": 5     # Bread (upkeep)
            }
            # Add more troops here...
        }
        return mapping[self.value]

    @property
    def id(self):
        """Magic / troop ID"""
        return self.data["id"]
