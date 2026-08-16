from enum import Enum

class Tool(Enum):
    """Tool definitions name -> id"""
    SCALING_LADDER = "scaling_ladder"
    MANTLET = "mantlet"
    SHIELD_WALL = "shield_wall"

    @property
    def data(self):
        """Full data for this tool"""
        mapping = {
            "scaling_ladder": {
                "id": 614,
                "wall_reduction": 10
            },
            "mantlet":{
                "id": 620,
                "range_reduction": 5
            },
            "shield_wall": {
                "id": 651,
                "range_reduction": 15
            }
        }
        return mapping[self.value]

    @property
    def id(self):
        """Tool ID"""
        return self.data["id"]
