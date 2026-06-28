from enum import Enum

class Kingdom(Enum):
    """Kingdom definitions - name → id mapping"""

    GREEN_KINGDOM = "green_kingdom"
    SAND_KINGDOM = "sand_kingdom"
    ICE_KINGDOM = "ice_kingdom"
    FIRE_KINGDOM = "fire_kingdom"
    STORM_KINGDOM = "storm_kingdom"
    BERIMOND_KINGDOM = "berimond_kingdom"

    @property
    def data(self):
        """Full data for this kingdom"""
        mapping = {
            "green_kingdom": {
                "id": 0,
                "name": "green_kingdom"
            },
            "sand_kingdom": {
                "id": 1,
                "name": "sand_kingdom"
            },
            "ice_kingdom": {
                "id": 2,
                "name": "ice_kingdom"
            },
            "fire_kingdom": {
                "id": 3,
                "name": "fire_kingdom"
            },
            "storm_kingdom": {
                "id": 4,
                "name": "storm_kingdom"
            },
            "berimond_kingdom": {
                "id":10,
                "name": "berimond_kingdom"
            }
        }
        return mapping[self.value]

    @property
    def id(self):
        """Kingdom ID"""
        return self.data["id"]
