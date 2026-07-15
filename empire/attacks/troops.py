# from enum import Enum

# class Troop(Enum):

#     VETERAN_DEMON_HORROR = "veteran_demon_horror"
#     DEMON_HORROR = "demon_horror"
#     VETERAN_DEATHLY_HORROR = "veteran_deathly_horror"
#     RENEGADE_KUNAI_THROWER = "renegade_kunai_thrower"
#     CROSSBOWMAN_OF_THE_KINGSGUARD = "crossbowman_of_the_kingsguard"
#     SHIELD_MAIDEN_0 = "shield_maiden_0"
#     VALKYRIE_RANGER_0 = "valkyrie_ranger_0"
#     SHIELD_MAIDEN_10 = "shield_maiden_10"
#     DIREWOLF = "direwolf"
#     RENEGADE_SAI_WARRIOR = "renegade_sai_warrior"

#     @property
#     def data(self):
#         """All stats for this troop"""
#         mapping = {
#             "veteran_demon_horror": {
#                 "id": 9,
#                 "name": "veteran_demon_horror",
#                 "attack_power": 200,
#                 "melee_defence": 21,
#                 "ranged_defence": 6,
#                 "loot_capacity": 45,
#                 "travel_speed": 30,
#                 "food_consumption": 5
#             },
#             "demon_horror": {
#                 "id": 714,
#                 "name": "demon_horror",
#                 "attack_power": 185,      # Red sword (main attack)
#                 "melee_defence": 19,      # Blue sword
#                 "ranged_defence": 5,      # Bow
#                 "loot_capacity": 35,      # Bag icon
#                 "travel_speed": 28,       # Hourglass (movement / time)
#                 "food_consumption": 5     # Bread (upkeep)
#             },
#             "veteran_deathly_horror": {
#                 "id": 10,
#                 "name": "veteran_deathly_horror",
#                 "attack_power": 175,
#                 "melee_defence": 17,
#                 "ranged_defence": 26,
#                 "loot_capacity": 50,
#                 "travel_speed": 30,
#                 "food_consumption": 5
#             },
#             "renegade_sai_warrior": {
#                 "id": 34,
#                 "name": "renegade_sai_warrior",
#                 "attack_power": 160,
#                 "melee_defence": 20,
#                 "ranged_defence": 7,
#                 "loot_capacity": 29,
#                 "travel_speed": 50,
#                 "food_consumption": 4
#             },
#             "renegade_kunai_thrower": {
#                 "id": 35,
#                 "name": "renegade_kunai_thrower",
#                 "attack_power": 148,
#                 "melee_defence": 18,
#                 "ranged_defence": 27,
#                 "loot_capacity": 25,
#                 "travel_speed": 50,
#                 "food_consumption": 4
#             },
#             "crossbowman_of_the_kingsguard": {
#                 "id": 664,
#                 "name": "crossbowman_of_the_kingsguard",
#                 "attack_power": 121,
#                 "melee_defence": 14,
#                 "ranged_defence": 23,
#                 "loot_capacity": 29,
#                 "travel_speed": 28,
#                 "food_consumption": 4
#             },
#             "shield_maiden_0": {
#                 "id": 999,
#                 "name": "shield_maiden_0",
#                 "attack_power": 225,
#                 "melee_defence": 28,
#                 "ranged_defence": 10,
#                 "loot_capacity": 43,
#                 "travel_speed": 34,
#                 "food_consumption": 2
#             },
#             "valkyrie_ranger_0": {
#                 "id": 216,
#                 "name": "valkyrie_ranger_0",
#                 "attack_power": 310,
#                 "melee_defence": 28,
#                 "ranged_defence": 48,
#                 "loot_capacity": 60,
#                 "travel_speed": 34,
#                 "food_consumption": 2
#             },
#             "shield_maiden_10": {
#                 "id": 215,
#                 "name": "shield_maiden_10",
#                 "attack_power": 325,
#                 "melee_defence": 41,
#                 "ranged_defence": 15,
#                 "loot_capacity": 63,
#                 "travel_speed": 34,
#                 "food_consumption": 2
#             },
#             "direwolf": {
#                 "id": 277,
#                 "name": "direwolf",
#                 "attack_power": 97,
#                 "melee_defence": 40,
#                 "ranged_defence": 0,
#                 "loot_capacity": 4,
#                 "travel_speed": 100,
#                 "food_consumption": 4
#             },
#         }
#         return mapping[self.value]

#     @property
#     def id(self):
#         """Magic / troop ID"""
#         return self.data["id"]


from enum import Enum


class Troop(Enum):

    VETERAN_DEMON_HORROR = "veteran_demon_horror"
    DEMON_HORROR = "demon_horror"
    VETERAN_DEATHLY_HORROR = "veteran_deathly_horror"
    RENEGADE_KUNAI_THROWER = "renegade_kunai_thrower"
    CROSSBOWMAN = "crossbowman"
    CROSSBOWMAN_OF_THE_KINGSGUARD = "crossbowman_of_the_kingsguard"
    SHIELD_MAIDEN_0 = "shield_maiden_0"
    SHIELD_MAIDEN_10 = "shield_maiden_10"
    VALKYRIE_RANGER_0 = "valkyrie_ranger_0"
    VALKYRIE_RANGER_10 = "valkyrie_ranger_10"
    DIREWOLF = "direwolf"
    RENEGADE_SAI_WARRIOR = "renegade_sai_warrior"
    RELIC_SHORTBOWMAN_0 = "relic_shortbowman_0"
    

    @property
    def data(self):
        """All stats for this troop"""
        mapping = {
            "veteran_demon_horror": {
                "id": 9,
                "name": "veteran_demon_horror",
                "attack_power": 200,
                "melee_defence": 21,
                "ranged_defence": 6,
                "loot_capacity": 45,
                "travel_speed": 30,
                "food_consumption": 5
            },
            "valkyrie_ranger_10": {
                "id": 216,
                "name": "valkyrie_ranger_10",
                "attack_power": 310,
                "melee_defence": 28,
                "ranged_defence": 48,
                "loot_capacity": 60,
                "travel_speed": 34,
                "food_consumption": 0,
                "mead_consumption": 2},
            
            "crossbowman": {
                "id": 607,
                "name": "crossbowman",
                "attack_power": 36,
                "melee_defence": 6,
                "ranged_defence": 36,
                "loot_capacity": 22,
                "travel_speed": 73,
                "food_consumption": 2,
                "mead_consumption": 0},
            "demon_horror": {
                "id": 714,
                "name": "demon_horror",
                "attack_power": 185,      # Red sword (main attack)
                "melee_defence": 19,      # Blue sword
                "ranged_defence": 5,      # Bow
                "loot_capacity": 35,      # Bag icon
                "travel_speed": 28,       # Hourglass (movement / time)
                "food_consumption": 5     # Bread (upkeep)
            },
            "veteran_deathly_horror": {
                "id": 10,
                "name": "veteran_deathly_horror",
                "attack_power": 175,
                "melee_defence": 17,
                "ranged_defence": 26,
                "loot_capacity": 50,
                "travel_speed": 30,
                "food_consumption": 5
            },
            "renegade_kunai_thrower": {
                "id": 35,
                "name": "renegade_kunai_thrower",
                "attack_power": 148,
                "melee_defence": 18,
                "ranged_defence": 27,
                "loot_capacity": 25,
                "travel_speed": 50,
                "food_consumption": 4
            },
            "crossbowman_of_the_kingsguard": {
                "id": 664,
                "name": "crossbowman_of_the_kingsguard",
                "attack_power": 121,
                "melee_defence": 14,
                "ranged_defence": 23,
                "loot_capacity": 29,
                "travel_speed": 28,
                "food_consumption": 4
            },
            "shield_maiden_0": {
                "id": 999,
                "name": "shield_maiden_0",
                "attack_power": 225,
                "melee_defence": 28,
                "ranged_defence": 10,
                "loot_capacity": 43,
                "travel_speed": 34,
                "food_consumption": 2
            },
            "relic_shortbowman_0": {
                "id": 149,
                "name": "relic_shortbowman_0",
                "attack_power": 151,
                "melee_defence": 16,
                "ranged_defence": 24,
                "loot_capacity": 35,
                "travel_speed": 30,
                "food_consumption": 5
            },
            "valkyrie_ranger_0": {
                "id": 216,
                "name": "valkyrie_ranger_0",
                "attack_power": 310,
                "melee_defence": 28,
                "ranged_defence": 48,
                "loot_capacity": 60,
                "travel_speed": 34,
                "food_consumption": 2
            },
            "shield_maiden_10": {
                "id": 215,
                "name": "shield_maiden_10",
                "attack_power": 325,
                "melee_defence": 41,
                "ranged_defence": 15,
                "loot_capacity": 63,
                "travel_speed": 34,
                "food_consumption": 2
            },
            "direwolf": {
                "id": 277,
                "name": "direwolf",
                "attack_power": 97,
                "melee_defence": 40,
                "ranged_defence": 0,
                "loot_capacity": 4,
                "travel_speed": 100,
                "food_consumption": 4
            },
            "renegade_sai_warrior": {
                "id": 34,
                "name": "renegade_sai_warrior",
                "attack_power": 160,
                "melee_defence": 20,
                "ranged_defence": 7,
                "loot_capacity": 29,
                "travel_speed": 50,
                "food_consumption": 4
            },
        }
        return mapping[self.value]

    @property
    def id(self):
        """Magic / troop ID"""
        return self.data["id"]


