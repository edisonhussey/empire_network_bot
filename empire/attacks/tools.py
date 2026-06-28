from enum import Enum

class Tool(Enum):
    """Tool definitions name -> id"""
    SCALING_LADDER = "scaling_ladder"

    @property
    def data(self):
        """Full data for this tool"""
        mapping = {
            "scaling_ladder": {
                "id": 614,
                "wall_reduction": 10
            }
        }
        return mapping[self.value]

    @property
    def id(self):
        """Tool ID"""
        return self.data["id"]
