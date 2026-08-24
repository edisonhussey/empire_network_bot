from enum import Enum

class Tool(Enum):
    """Tool definitions name -> id"""
    KHAN_CHEST = "khan_chest"
    SPIKED_SHIELD = "spiked_shield"
    THE_WOLFS_HOWL = "the_wolfs_howl"
    TONNELON = "tonnelon"
    THE_SPLENDOR_OF_BERIMOND = "the_splendor_of_berimond"
    THE_TREASURE_OF_BERIMOND = "the_treasure_of_berimond"
    BANNER_OF_TRIUMPH = "banner_of_triumph"
    GATE_STORMER = "gate_stormer"
    FOLDING_LADDER = "folding_ladder"
    SWIVELING_SHIELD = "swiveling_shield"
    SMOKE_BOMB = "smoke_bomb"
    ARROW_RAIN_BALLISTA = "arrow_rain_ballista"
    CASE_OF_SAMURAI_TOKENS = "case_of_samurai_tokens"
    CHEST_OF_SAMURAI_TOKENS = "chest_of_samurai_tokens"
    KARIMATA_ARROWHEADS = "karimata_arrowheads"
    THROWING_NAILS = "throwing_nails"
    VAMPIRE_BATS = "vampire_bats"
    BUBBLING_CAULDRON = "bubbling_cauldron"
    WHALEBONE_RAM = "whalebone_ram"
    BONE_LADDER = "bone_ladder"
    SHIELD_SLED = "shield_sled"
    BALLAST_STONE = "ballast_stone"
    HEAVY_BALLAST_STONE = "heavy_ballast_stone"
    STAR_SPANGLED_BANNER_60 = "star_spangled_banner_60"
    BANNER_OF_THE_FROST_WARRIORS = "banner_of_the_frost_warriors"
    NOMAD_PIGGY_BANK = "nomad_piggy_bank"
    SAMURAI_PIGGY_BANK = "samurai_piggy_bank"
    THE_WINTER_KINGS_BANNER = "the_winter_kings_banner"
    ROYAL_COIN_CASE = "royal_coin_case"
    PREMIUM_ROYAL_COIN_CASE = "premium_royal_coin_case"
    PREMIUM_PIGGY_BANK = "premium_piggy_bank"
    CAVE_LADDER = "cave_ladder"
    STONE_SHIELD = "stone_shield"
    BANNER_OF_THE_UNDERWORLD = "banner_of_the_underworld"
    GRANITE_RAM = "granite_ram"
    GOLDEN_PIGGY_BANK = "golden_piggy_bank"
    HEROS_BANNER = "heros_banner"
    CRYSTAL_BALLISTA = "crystal_ballista"
    REPUTATION_BOOSTER = "reputation_booster"
    GOLDEN_REPUTATION_BOOSTER = "golden_reputation_booster"
    IRONWOOD_RAM = "ironwood_ram"
    VINE_LADDER = "vine_ladder"
    BARK_SHIELD = "bark_shield"
    FLAG_OF_THE_FOREST_GUARDIANS = "flag_of_the_forest_guardians"
    SOLDIERS_BANNER = "soldiers_banner"
    GOLDEN_ROYAL_COIN_CASE = "golden_royal_coin_case"
    ROYAL_TOKEN_CHEST = "royal_token_chest"
    PREMIUM_ROYAL_TOKEN_CHEST = "premium_royal_token_chest"
    GOLDEN_ROYAL_TOKEN_CHEST = "golden_royal_token_chest"
    BROADHEAD_ARROWS = "broadhead_arrows"
    QUICKLIME_BOMB = "quicklime_bomb"
    RAGE_BANNER_106 = "rage_banner_106"
    PLUNDERERS_CHEST = "plunderers_chest"
    PLUNDERERS_TRUNK = "plunderers_trunk"
    NOMADIC_MURDER_HOLE = "nomadic_murder_hole"
    BALISTRARIA = "balistraria"
    REINFORCED_PORTCULLIS = "reinforced_portcullis"
    INFERNO_MOAT = "inferno_moat"
    WOLVERINE_RAM_113 = "wolverine_ram_113"
    ELITE_TONNELON_114 = "elite_tonnelon_114"
    ONSLAUGHT_BRIDGE = "onslaught_bridge"
    ELITE_SPIKED_SHIELD = "elite_spiked_shield"
    GRIMS_SHIELD = "grims_shield"
    GRIMS_LADDER = "grims_ladder"
    GRIMS_RAM = "grims_ram"
    GRIMS_CROSSING = "grims_crossing"
    FLEET_SHIELD = "fleet_shield"
    FESTIVE_RAM = "festive_ram"
    CANDY_CANE_LADDER = "candy_cane_ladder"
    YULE_SHIELD = "yule_shield"
    BAUBLE_CROSSING = "bauble_crossing"
    RUNE_RAM = "rune_ram"
    VERTICAL_ASSAULT_APPARATUS = "vertical_assault_apparatus"
    INVADERS_FOOTBRIDGE = "invaders_footbridge"
    INFINITY_SHIELD = "infinity_shield"
    ELITE_NOMAD_RAM_141 = "elite_nomad_ram_141"
    ELITE_NOMAD_ROPE_LADDER_142 = "elite_nomad_rope_ladder_142"
    ELITE_NOMAD_SHIELD_143 = "elite_nomad_shield_143"
    ROYAL_BANNER = "royal_banner"
    KINGS_BANNER = "kings_banner"
    ROYAL_RAM = "royal_ram"
    KINGS_RAM = "kings_ram"
    ROYAL_LADDER = "royal_ladder"
    KINGS_LADDER = "kings_ladder"
    ROYAL_BRIDGE = "royal_bridge"
    KINGS_BRIDGE = "kings_bridge"
    ROYAL_SHIELD = "royal_shield"
    KINGS_SHIELD = "kings_shield"
    SILVER_KHAN_CHEST = "silver_khan_chest"
    GOLD_KHAN_CHEST = "gold_khan_chest"
    ROYAL_KHAN_CHEST = "royal_khan_chest"
    CASE_OF_THE_SHOGUN = "case_of_the_shogun"
    CHEST_OF_THE_SHOGUN = "chest_of_the_shogun"
    IMPROVED_FIRE_CART = "improved_fire_cart"
    IMPROVED_TREE_LADDER = "improved_tree_ladder"
    IMPROVED_ROLLING_SHIELD = "improved_rolling_shield"
    RAM_OF_THE_SHOGUN_170 = "ram_of_the_shogun_170"
    LADDER_OF_THE_SHOGUN_171 = "ladder_of_the_shogun_171"
    SHIELD_OF_THE_SHOGUN_172 = "shield_of_the_shogun_172"
    THE_DIGNITY_OF_BERIMOND = "the_dignity_of_berimond"
    INSULATING_MAT_179 = "insulating_mat_179"
    HURLING_ROCKS_180 = "hurling_rocks_180"
    SHARPENED_STAKES_181 = "sharpened_stakes_181"
    FLAMING_ARROWS_182 = "flaming_arrows_182"
    VISCOUNTS_BANNER = "viscounts_banner"
    EMPERORS_BANNER = "emperors_banner"
    THE_RISE_OF_BERIMOND = "the_rise_of_berimond"
    THE_SAVIOR_OF_BERIMOND = "the_savior_of_berimond"
    KNIGHT_OF_KHAN_CHEST = "knight_of_khan_chest"
    CROWN_OF_KHAN_CHEST = "crown_of_khan_chest"
    FORTIFIED_RAM_LVL_1 = "fortified_ram_lvl_1"
    GLORY_TOWER_LVL_1 = "glory_tower_lvl_1"
    HOOKSHOT_CANNON_LVL_1 = "hookshot_cannon_lvl_1"
    HWACHA_LVL_1 = "hwacha_lvl_1"
    SIEGE_MORTAR_LVL_1 = "siege_mortar_lvl_1"
    THUNDER_CRASH_BOMB_LVL_1 = "thunder_crash_bomb_lvl_1"
    TREBUCHET_LVL_1 = "trebuchet_lvl_1"
    FIELD_CANNON_LVL_1 = "field_cannon_lvl_1"
    WATER_MINE_LVL_1 = "water_mine_lvl_1"
    SPIKE_BOARD_LVL_1 = "spike_board_lvl_1"
    SHRAPNEL_BOMB_LVL_1 = "shrapnel_bomb_lvl_1"
    ORGAN_CANNON_LVL_1 = "organ_cannon_lvl_1"
    HAND_CANNON_LVL_1 = "hand_cannon_lvl_1"
    ASSAULT_FLAME_THROWER_LVL_1 = "assault_flame_thrower_lvl_1"
    WARWAGON_LVL_1 = "warwagon_lvl_1"
    LARGE_NOMAD_TABLET_CHEST = "large_nomad_tablet_chest"
    HUGE_NOMAD_TABLET_CHEST = "huge_nomad_tablet_chest"
    LARGE_SAMURAI_TOKEN_CHEST = "large_samurai_token_chest"
    HUGE_SAMURAI_TOKEN_CHEST = "huge_samurai_token_chest"
    STAR_SPANGLED_BANNER_408 = "star_spangled_banner_408"
    SPEAR_TRAP_LVL_1 = "spear_trap_lvl_1"
    FIRE_TRAP_LVL_1 = "fire_trap_lvl_1"
    WOODEN_HOARDING_LVL_1 = "wooden_hoarding_lvl_1"
    MOBILE_CAULDRON_LVL_1 = "mobile_cauldron_lvl_1"
    EXPLOSIVE_ARROWS_LVL_1 = "explosive_arrows_lvl_1"
    HIGH_QUEENS_BANNER = "high_queens_banner"
    HIGH_KINGS_BANNER = "high_kings_banner"
    SUPREME_MONARCHS_BANNER = "supreme_monarchs_banner"
    GIANT_NOMAD_TABLET_CHEST = "giant_nomad_tablet_chest"
    ENORMOUS_NOMAD_TABLET_CHEST = "enormous_nomad_tablet_chest"
    COLOSSAL_NOMAD_TABLET_CHEST = "colossal_nomad_tablet_chest"
    GIANT_SAMURAI_TOKEN_CHEST = "giant_samurai_token_chest"
    ENORMOUS_SAMURAI_TOKEN_CHEST = "enormous_samurai_token_chest"
    COLOSSAL_SAMURAI_TOKEN_CHEST = "colossal_samurai_token_chest"
    RAM_OF_THE_SHOGUN_557 = "ram_of_the_shogun_557"
    SHIELD_OF_THE_SHOGUN_558 = "shield_of_the_shogun_558"
    LADDER_OF_THE_SHOGUN_559 = "ladder_of_the_shogun_559"
    ELITE_NOMAD_RAM_560 = "elite_nomad_ram_560"
    ELITE_NOMAD_ROPE_LADDER_561 = "elite_nomad_rope_ladder_561"
    ELITE_NOMAD_SHIELD_562 = "elite_nomad_shield_562"
    RAGE_BANNER_563 = "rage_banner_563"
    WOLVERINE_RAM_564 = "wolverine_ram_564"
    ELITE_TONNELON_565 = "elite_tonnelon_565"
    INVADER_MASK_566 = "invader_mask_566"
    RAID_LADDER_60 = "raid_ladder_60"
    EXALTED_RAID_LADDER_250 = "exalted_raid_ladder_250"
    RAID_RAM_60 = "raid_ram_60"
    EXALTED_RAID_RAM_250 = "exalted_raid_ram_250"
    RAID_DOMINATOR_LADDER = "raid_dominator_ladder"
    MASTER_RIFT_DOMINATOR_LADDER = "master_rift_dominator_ladder"
    RAID_DOMINATOR_RAM = "raid_dominator_ram"
    MASTER_RIFT_DOMINATOR_RAM = "master_rift_dominator_ram"
    RAID_DOMINATOR_BANNER = "raid_dominator_banner"
    VETERAN_DOMINATOR_BANNER = "veteran_dominator_banner"
    MASTER_DOMINATOR_BANNER = "master_dominator_banner"
    GRAVEGUARD_POTION = "graveguard_potion"
    GREAT_GRAVEGUARD_POTION = "great_graveguard_potion"
    PREMIUM_GRAVEGUARD_POTION = "premium_graveguard_potion"
    RAID_LADDER_80 = "raid_ladder_80"
    RAID_LADDER_125 = "raid_ladder_125"
    RAID_LADDER_200 = "raid_ladder_200"
    EXALTED_RAID_LADDER_340 = "exalted_raid_ladder_340"
    EXALTED_RAID_LADDER_500 = "exalted_raid_ladder_500"
    RAID_RAM_80 = "raid_ram_80"
    RAID_RAM_125 = "raid_ram_125"
    RAID_RAM_200 = "raid_ram_200"
    EXALTED_RAID_RAM_340 = "exalted_raid_ram_340"
    EXALTED_RAID_RAM_500 = "exalted_raid_ram_500"
    BATTERING_RAM = "battering_ram"
    SCALING_LADDER = "scaling_ladder"
    WOOD_BUNDLE = "wood_bundle"
    MANTLET = "mantlet"
    PORTCULLIS = "portcullis"
    BAMBOO_LADDER = "bamboo_ladder"
    ARROW_SLIT = "arrow_slit"
    FIRE_MOAT = "fire_moat"
    CASTLE_GATE_REINFORCEMENT = "castle_gate_reinforcement"
    MURDER_HOLE = "murder_hole"
    FLAMING_ARROWS_629 = "flaming_arrows_629"
    DUMMY_MELEE_SOLDIERS = "dummy_melee_soldiers"
    DUMMY_RANGED_SOLDIERS = "dummy_ranged_soldiers"
    SHARPENED_STAKES_634 = "sharpened_stakes_634"
    CAST_IRON_MANTLET = "cast_iron_mantlet"
    HURLING_ROCKS_637 = "hurling_rocks_637"
    MOON_STORM_BANNER = "moon_storm_banner"
    DUMMY_ARCHERS = "dummy_archers"
    SIEGE_TOWER = "siege_tower"
    IRON_RAM = "iron_ram"
    ASSAULT_BRIDGE = "assault_bridge"
    GRIMACING_SHIELD = "grimacing_shield"
    INSULATING_MAT_644 = "insulating_mat_644"
    BULWARK = "bulwark"
    SWAMP_SNAPPER = "swamp_snapper"
    TAR_PITCH_KETTLE = "tar_pitch_kettle"
    HEAVY_RAM = "heavy_ram"
    BREACHING_TOWER = "breaching_tower"
    BOULDERS = "boulders"
    SHIELD_WALL = "shield_wall"
    LOOT_SACK = "loot_sack"
    LOOT_CART = "loot_cart"
    BANNER = "banner"
    WAR_BANNER = "war_banner"
    LARGE_BALLISTA = "large_ballista"
    LIME_POWDER_BOMB = "lime_powder_bomb"
    BODKIN_ARROWHEADS = "bodkin_arrowheads"
    TURTLE_RAM = "turtle_ram"
    GRAPPLING_HOOKS = "grappling_hooks"
    HORSETAIL_STANDARD = "horsetail_standard"
    DUMMY_PEASANTS = "dummy_peasants"
    TRANSPORT_YAK = "transport_yak"
    WAR_HORN = "war_horn"
    FIRE_CART = "fire_cart"
    ROLLING_SHIELD = "rolling_shield"
    TREE_TRUNK_LADDER = "tree_trunk_ladder"
    TORCHES = "torches"
    POISON_ARROWS = "poison_arrows"
    VETERAN_BANNER = "veteran_banner"
    THORN_LADDER = "thorn_ladder"
    WEDGE_SHIELD = "wedge_shield"
    BANNER_OF_THE_DEMON_SLAYERS = "banner_of_the_demon_slayers"
    BALLISTA = "ballista"
    BOARDING_LADDER = "boarding_ladder"
    SHARK_SHIELD = "shark_shield"
    KRAKEN_BANNER = "kraken_banner"
    NAUTICAL_BALLISTA = "nautical_ballista"
    INVADER_RAM = "invader_ram"
    INVADER_LADDER = "invader_ladder"
    INVADER_BRIDGE = "invader_bridge"
    INVADER_MASK_773 = "invader_mask_773"
    BANNER_OF_THE_FOREIGN_INVADERS = "banner_of_the_foreign_invaders"
    THE_CLAW = "the_claw"
    PAW_LADDER = "paw_ladder"
    FANGED_BRIDGE = "fanged_bridge"
    BERIMOND_SHIELD = "berimond_shield"
    THE_GLORY_OF_BERIMOND = "the_glory_of_berimond"
    THE_PRIDE_OF_BERIMOND = "the_pride_of_berimond"
    BRONZE_MAGMATIC_CHRONO_VIAL_LEFT = "bronze_magmatic_chrono_vial_left"
    BRONZE_MAGMATIC_CHRONO_VIAL_GATE = "bronze_magmatic_chrono_vial_gate"
    BRONZE_MAGMATIC_CHRONO_VIAL_RIGHT = "bronze_magmatic_chrono_vial_right"
    BRONZE_MAGMATIC_CHRONO_VIAL_ALL_SIDES = "bronze_magmatic_chrono_vial_all_sides"
    SILVER_MAGMATIC_CHRONO_VIAL_LEFT = "silver_magmatic_chrono_vial_left"
    SILVER_MAGMATIC_CHRONO_VIAL_GATE = "silver_magmatic_chrono_vial_gate"
    SILVER_MAGMATIC_CHRONO_VIAL_RIGHT = "silver_magmatic_chrono_vial_right"
    SILVER_MAGMATIC_CHRONO_VIAL_ALL_SIDES = "silver_magmatic_chrono_vial_all_sides"
    GOLD_MAGMATIC_CHRONO_VIAL_LEFT = "gold_magmatic_chrono_vial_left"
    GOLD_MAGMATIC_CHRONO_VIAL_GATE = "gold_magmatic_chrono_vial_gate"
    GOLD_MAGMATIC_CHRONO_VIAL_RIGHT = "gold_magmatic_chrono_vial_right"
    GOLD_MAGMATIC_CHRONO_VIAL_ALL_SIDES = "gold_magmatic_chrono_vial_all_sides"
    BRONZE_MAGMATIC_GRENADE_FLASK = "bronze_magmatic_grenade_flask"
    SILVER_MAGMATIC_GRENADE_FLASK = "silver_magmatic_grenade_flask"
    GOLD_MAGMATIC_GRENADE_FLASK = "gold_magmatic_grenade_flask"
    BRONZE_MAGMATIC_FLINTLOCK_GUN = "bronze_magmatic_flintlock_gun"
    SILVER_MAGMATIC_FLINTLOCK_GUN = "silver_magmatic_flintlock_gun"
    GOLD_MAGMATIC_FLINTLOCK_GUN = "gold_magmatic_flintlock_gun"
    BRONZE_OBSIDIAN_WEIGHTED_NET = "bronze_obsidian_weighted_net"
    SILVER_OBSIDIAN_WEIGHTED_NET = "silver_obsidian_weighted_net"
    GOLD_OBSIDIAN_WEIGHTED_NET = "gold_obsidian_weighted_net"
    BRONZE_OBSIDIAN_SIEGE_SHIELD = "bronze_obsidian_siege_shield"
    SILVER_OBSIDIAN_SIEGE_SHIELD = "silver_obsidian_siege_shield"
    GOLD_OBSIDIAN_SIEGE_SHIELD = "gold_obsidian_siege_shield"
    THROWING_NAILS_LVL_1 = "throwing_nails_lvl_1"
    KARIMATA_ARROWHEADS_LVL_1 = "karimata_arrowheads_lvl_1"
    ARROW_RAIN_BALLISTA_LVL_1 = "arrow_rain_ballista_lvl_1"
    SMOKE_BOMB_LVL_1 = "smoke_bomb_lvl_1"
    LIME_POWDER_BOMB_LVL_1 = "lime_powder_bomb_lvl_1"
    BODKIN_ARROWHEADS_LVL_1 = "bodkin_arrowheads_lvl_1"
    BRONZE_SPOREGUARD_CHRONO_VIAL_LEFT = "bronze_sporeguard_chrono_vial_left"
    BRONZE_SPOREGUARD_CHRONO_VIAL_GATE = "bronze_sporeguard_chrono_vial_gate"
    BRONZE_SPOREGUARD_CHRONO_VIAL_RIGHT = "bronze_sporeguard_chrono_vial_right"
    BRONZE_SPOREGUARD_CHRONO_VIAL_ALL_SIDES = "bronze_sporeguard_chrono_vial_all_sides"
    SILVER_SPOREGUARD_CHRONO_VIAL_LEFT = "silver_sporeguard_chrono_vial_left"
    SILVER_SPOREGUARD_CHRONO_VIAL_GATE = "silver_sporeguard_chrono_vial_gate"
    SILVER_SPOREGUARD_CHRONO_VIAL_RIGHT = "silver_sporeguard_chrono_vial_right"
    SILVER_SPOREGUARD_CHRONO_VIAL_ALL_SIDES = "silver_sporeguard_chrono_vial_all_sides"
    GOLD_SPOREGUARD_CHRONO_VIAL_LEFT = "gold_sporeguard_chrono_vial_left"
    GOLD_SPOREGUARD_CHRONO_VIAL_GATE = "gold_sporeguard_chrono_vial_gate"
    GOLD_SPOREGUARD_CHRONO_VIAL_RIGHT = "gold_sporeguard_chrono_vial_right"
    GOLD_SPOREGUARD_CHRONO_VIAL_ALL_SIDES = "gold_sporeguard_chrono_vial_all_sides"
    BRONZE_SPOREBANE_GRENADE_FLASK = "bronze_sporebane_grenade_flask"
    SILVER_SPOREBANE_GRENADE_FLASK = "silver_sporebane_grenade_flask"
    GOLD_SPOREBANE_GRENADE_FLASK = "gold_sporebane_grenade_flask"
    BRONZE_SPOREBANE_FLINTLOCK_GUN = "bronze_sporebane_flintlock_gun"
    SILVER_SPOREBANE_FLINTLOCK_GUN = "silver_sporebane_flintlock_gun"
    GOLD_SPOREBANE_FLINTLOCK_GUN = "gold_sporebane_flintlock_gun"

    @property
    def data(self):
        """Full data for this tool"""
        mapping = {
            "khan_chest": {
                "id": 1,
                "khan_tablet_bonus": 3
            },
            "spiked_shield": {
                "id": 2,
                "range_reduction": 15,
                "glory_points": 2
            },
            "the_wolfs_howl": {
                "id": 3,
                "melee_attack": 1,
                "glory_points": 2
            },
            "tonnelon": {
                "id": 4,
                "wall_reduction": 20,
                "glory_points": 2
            },
            "the_splendor_of_berimond": {
                "id": 16,
                "gallantry_points_bonus": 2
            },
            "the_treasure_of_berimond": {
                "id": 17,
                "gallantry_points_bonus": 3
            },
            "banner_of_triumph": {
                "id": 24,
                "glory_points": 4
            },
            "gate_stormer": {
                "id": 25,
                "gate_reduction": 25,
                "samurai_tokens_bonus": 3
            },
            "folding_ladder": {
                "id": 26,
                "wall_reduction": 25,
                "samurai_tokens_bonus": 3
            },
            "swiveling_shield": {
                "id": 27,
                "range_reduction": 20,
                "samurai_tokens_bonus": 3
            },
            "smoke_bomb": {
                "id": 28,
                "melee_attack": 30,
                "samurai_tokens_bonus": 3
            },
            "arrow_rain_ballista": {
                "id": 29,
                "ranged_attack": 30,
                "samurai_tokens_bonus": 3
            },
            "case_of_samurai_tokens": {
                "id": 30,
                "samurai_tokens_bonus": 3
            },
            "chest_of_samurai_tokens": {
                "id": 31,
                "samurai_tokens_bonus": 4
            },
            "karimata_arrowheads": {
                "id": 32,
                "ranged_defence": 50
            },
            "throwing_nails": {
                "id": 33,
                "melee_defence": 33
            },
            "vampire_bats": {
                "id": 44,
                "range_reduction": 10
            },
            "bubbling_cauldron": {
                "id": 45,
                "wall_protection": 40
            },
            "whalebone_ram": {
                "id": 53,
                "gate_reduction": 15
            },
            "bone_ladder": {
                "id": 54,
                "wall_reduction": 15
            },
            "shield_sled": {
                "id": 55,
                "range_reduction": 10
            },
            "ballast_stone": {
                "id": 56
            },
            "heavy_ballast_stone": {
                "id": 57
            },
            "star_spangled_banner_60": {
                "id": 60,
                "glory_points": 2
            },
            "banner_of_the_frost_warriors": {
                "id": 61,
                "experience_points_bonus": 20
            },
            "nomad_piggy_bank": {
                "id": 62,
                "experience_points_bonus": 1,
                "coins": 1
            },
            "samurai_piggy_bank": {
                "id": 63,
                "experience_points_bonus": 1,
                "coins": 1
            },
            "the_winter_kings_banner": {
                "id": 64,
                "glory_points": 3
            },
            "royal_coin_case": {
                "id": 65
            },
            "premium_royal_coin_case": {
                "id": 66,
                "glory_points": 3,
                "khan_tablet_bonus": 2
            },
            "premium_piggy_bank": {
                "id": 67,
                "experience_points_bonus": 2,
                "coins": 2
            },
            "cave_ladder": {
                "id": 69,
                "wall_reduction": 25
            },
            "stone_shield": {
                "id": 70,
                "range_reduction": 20
            },
            "banner_of_the_underworld": {
                "id": 71,
                "melee_attack": 30
            },
            "granite_ram": {
                "id": 72,
                "gate_reduction": 25
            },
            "golden_piggy_bank": {
                "id": 73,
                "experience_points_bonus": 3,
                "coins": 5
            },
            "heros_banner": {
                "id": 77,
                "glory_points": 3
            },
            "crystal_ballista": {
                "id": 80,
                "ranged_attack": 30
            },
            "reputation_booster": {
                "id": 81,
                "reputation_points_bonus": 1
            },
            "golden_reputation_booster": {
                "id": 82,
                "reputation_points_bonus": 4
            },
            "ironwood_ram": {
                "id": 87,
                "gate_reduction": 15
            },
            "vine_ladder": {
                "id": 88,
                "wall_reduction": 15
            },
            "bark_shield": {
                "id": 89,
                "range_reduction": 10
            },
            "flag_of_the_forest_guardians": {
                "id": 90,
                "experience_points_bonus": 20
            },
            "soldiers_banner": {
                "id": 91,
                "experience_points_bonus": 10
            },
            "golden_royal_coin_case": {
                "id": 94,
                "glory_points": 5,
                "khan_tablet_bonus": 3
            },
            "royal_token_chest": {
                "id": 95,
                "glory_points": 4,
                "khan_tablet_bonus": 1
            },
            "premium_royal_token_chest": {
                "id": 96,
                "glory_points": 5,
                "khan_tablet_bonus": 2
            },
            "golden_royal_token_chest": {
                "id": 97,
                "glory_points": 6,
                "khan_tablet_bonus": 3
            },
            "broadhead_arrows": {
                "id": 104,
                "ranged_defence": 60
            },
            "quicklime_bomb": {
                "id": 105,
                "melee_defence": 43
            },
            "rage_banner_106": {
                "id": 106,
                "rage_bonus": 1
            },
            "plunderers_chest": {
                "id": 107,
                "khan_tablet_bonus": 1,
                "rage_bonus": 1
            },
            "plunderers_trunk": {
                "id": 108,
                "khan_medal_bonus": 7
            },
            "nomadic_murder_hole": {
                "id": 109,
                "wall_protection": 50,
                "khan_medal_bonus": 3
            },
            "balistraria": {
                "id": 110,
                "ranged_defence": 70,
                "khan_medal_bonus": 3
            },
            "reinforced_portcullis": {
                "id": 111,
                "gate_protection": 75,
                "khan_medal_bonus": 3
            },
            "inferno_moat": {
                "id": 112,
                "moat_protection": 110,
                "khan_medal_bonus": 3
            },
            "wolverine_ram_113": {
                "id": 113,
                "gate_reduction": 20,
                "glory_points": 5
            },
            "elite_tonnelon_114": {
                "id": 114,
                "wall_reduction": 20,
                "glory_points": 5
            },
            "onslaught_bridge": {
                "id": 115,
                "moat_reduction": 15,
                "glory_points": 5
            },
            "elite_spiked_shield": {
                "id": 116,
                "range_reduction": 15,
                "glory_points": 5
            },
            "grims_shield": {
                "id": 121,
                "range_reduction": 10
            },
            "grims_ladder": {
                "id": 122,
                "wall_reduction": 15
            },
            "grims_ram": {
                "id": 123,
                "gate_reduction": 15
            },
            "grims_crossing": {
                "id": 124,
                "moat_reduction": 15,
                "glory_points": 5
            },
            "fleet_shield": {
                "id": 125,
                "range_reduction": 10
            },
            "festive_ram": {
                "id": 128,
                "gate_reduction": 15
            },
            "candy_cane_ladder": {
                "id": 129,
                "wall_reduction": 15
            },
            "yule_shield": {
                "id": 130,
                "range_reduction": 10
            },
            "bauble_crossing": {
                "id": 131,
                "moat_reduction": 15
            },
            "rune_ram": {
                "id": 137,
                "gate_reduction": 20
            },
            "vertical_assault_apparatus": {
                "id": 138,
                "wall_reduction": 20
            },
            "invaders_footbridge": {
                "id": 139,
                "moat_reduction": 15
            },
            "infinity_shield": {
                "id": 140,
                "range_reduction": 15
            },
            "elite_nomad_ram_141": {
                "id": 141,
                "gate_reduction": 25,
                "khan_tablet_bonus": 5
            },
            "elite_nomad_rope_ladder_142": {
                "id": 142,
                "wall_reduction": 25,
                "khan_tablet_bonus": 5
            },
            "elite_nomad_shield_143": {
                "id": 143,
                "range_reduction": 20,
                "khan_tablet_bonus": 5
            },
            "royal_banner": {
                "id": 152,
                "glory_points": 6
            },
            "kings_banner": {
                "id": 153,
                "glory_points": 8
            },
            "royal_ram": {
                "id": 154,
                "gate_reduction": 20,
                "glory_points": 6
            },
            "kings_ram": {
                "id": 155,
                "gate_reduction": 20,
                "glory_points": 8
            },
            "royal_ladder": {
                "id": 156,
                "wall_reduction": 20,
                "glory_points": 6
            },
            "kings_ladder": {
                "id": 157,
                "wall_reduction": 20,
                "glory_points": 8
            },
            "royal_bridge": {
                "id": 158,
                "moat_reduction": 15,
                "glory_points": 6
            },
            "kings_bridge": {
                "id": 159,
                "moat_reduction": 15,
                "glory_points": 8
            },
            "royal_shield": {
                "id": 160,
                "range_reduction": 15,
                "glory_points": 6
            },
            "kings_shield": {
                "id": 161,
                "range_reduction": 15,
                "glory_points": 8
            },
            "silver_khan_chest": {
                "id": 162,
                "khan_tablet_bonus": 4
            },
            "gold_khan_chest": {
                "id": 163,
                "khan_tablet_bonus": 5
            },
            "royal_khan_chest": {
                "id": 164,
                "khan_tablet_bonus": 6
            },
            "case_of_the_shogun": {
                "id": 165,
                "samurai_tokens_bonus": 5
            },
            "chest_of_the_shogun": {
                "id": 166,
                "samurai_tokens_bonus": 6
            },
            "improved_fire_cart": {
                "id": 167,
                "gate_reduction": 25,
                "khan_tablet_bonus": 6
            },
            "improved_tree_ladder": {
                "id": 168,
                "wall_reduction": 25,
                "khan_tablet_bonus": 6
            },
            "improved_rolling_shield": {
                "id": 169,
                "range_reduction": 20,
                "khan_tablet_bonus": 6
            },
            "ram_of_the_shogun_170": {
                "id": 170,
                "gate_reduction": 25,
                "samurai_tokens_bonus": 5
            },
            "ladder_of_the_shogun_171": {
                "id": 171,
                "wall_reduction": 25,
                "samurai_tokens_bonus": 5
            },
            "shield_of_the_shogun_172": {
                "id": 172,
                "range_reduction": 20,
                "samurai_tokens_bonus": 5
            },
            "the_dignity_of_berimond": {
                "id": 178,
                "gallantry_points_bonus": 1
            },
            "insulating_mat_179": {
                "id": 179,
                "gate_protection": 100
            },
            "hurling_rocks_180": {
                "id": 180,
                "wall_protection": 100
            },
            "sharpened_stakes_181": {
                "id": 181,
                "moat_protection": 100
            },
            "flaming_arrows_182": {
                "id": 182,
                "ranged_defence": 100
            },
            "viscounts_banner": {
                "id": 239,
                "glory_points": 9
            },
            "emperors_banner": {
                "id": 240,
                "glory_points": 10
            },
            "the_rise_of_berimond": {
                "id": 241,
                "gallantry_points_bonus": 5
            },
            "the_savior_of_berimond": {
                "id": 242,
                "gallantry_points_bonus": 6
            },
            "knight_of_khan_chest": {
                "id": 243,
                "khan_tablet_bonus": 7
            },
            "crown_of_khan_chest": {
                "id": 244,
                "khan_tablet_bonus": 8
            },
            "fortified_ram_lvl_1": {
                "id": 245,
                "range_reduction": 15,
                "gate_reduction": 30,
                "tool_limit_per_wave": 5
            },
            "glory_tower_lvl_1": {
                "id": 256,
                "range_reduction": 20,
                "glory_points": 1,
                "tool_limit_per_wave": 20
            },
            "hookshot_cannon_lvl_1": {
                "id": 267,
                "melee_attack": 1,
                "wall_reduction": 25,
                "tool_limit_per_wave": 20
            },
            "hwacha_lvl_1": {
                "id": 278,
                "ranged_attack": 2,
                "moat_reduction": 20,
                "tool_limit_per_wave": 5
            },
            "siege_mortar_lvl_1": {
                "id": 289,
                "ranged_attack": 1,
                "wall_reduction": 25,
                "tool_limit_per_wave": 20
            },
            "thunder_crash_bomb_lvl_1": {
                "id": 300,
                "ranged_defence": 50,
                "gate_protection": 60
            },
            "trebuchet_lvl_1": {
                "id": 315,
                "wall_protection": 60,
                "moat_protection": 80
            },
            "field_cannon_lvl_1": {
                "id": 326,
                "ranged_defence": 30,
                "wall_protection": 50
            },
            "water_mine_lvl_1": {
                "id": 337,
                "melee_defence": 60,
                "moat_protection": 80
            },
            "spike_board_lvl_1": {
                "id": 348,
                "melee_defence": 15,
                "wall_protection": 70
            },
            "shrapnel_bomb_lvl_1": {
                "id": 359,
                "kills_a_number_of_melee_defenders_in_the_courtyard": 275,
                "increase_unit_attack_strength_in_the_courtyard": 1,
                "tool_limit_per_wave": 1
            },
            "organ_cannon_lvl_1": {
                "id": 370,
                "kills_a_number_of_ranged_defenders_in_the_courtyard": 275,
                "increase_unit_attack_strength_in_the_courtyard": 1,
                "tool_limit_per_wave": 1
            },
            "hand_cannon_lvl_1": {
                "id": 381,
                "kills_a_number_of_defenders_in_the_courtyard": 300,
                "increase_unit_attack_strength_in_the_courtyard": 1,
                "tool_limit_per_wave": 1
            },
            "assault_flame_thrower_lvl_1": {
                "id": 391,
                "increase_unit_attack_strength_in_the_courtyard": 20,
                "tool_limit_per_wave": 1
            },
            "warwagon_lvl_1": {
                "id": 401,
                "additional_waves": 1,
                "combat_strength_when_attacking": 1,
                "tool_limit_per_wave": 1
            },
            "large_nomad_tablet_chest": {
                "id": 404,
                "khan_tablet_bonus": 9
            },
            "huge_nomad_tablet_chest": {
                "id": 405,
                "khan_tablet_bonus": 10
            },
            "large_samurai_token_chest": {
                "id": 406,
                "samurai_tokens_bonus": 7
            },
            "huge_samurai_token_chest": {
                "id": 407,
                "samurai_tokens_bonus": 8
            },
            "star_spangled_banner_408": {
                "id": 408,
                "glory_points": 11
            },
            "spear_trap_lvl_1": {
                "id": 421,
                "kills_a_number_of_attackers_in_the_courtyard": 100,
                "strength_in_courtyard_when_defending": 1
            },
            "fire_trap_lvl_1": {
                "id": 431,
                "strength_in_courtyard_when_defending": 20
            },
            "wooden_hoarding_lvl_1": {
                "id": 441,
                "increase_the_wall_capacity_for_defenders": 20,
                "increase_strength_of_defense_units": 1
            },
            "mobile_cauldron_lvl_1": {
                "id": 451,
                "kills_a_number_of_melee_attackers_in_the_courtyard": 100,
                "strength_in_courtyard_when_defending": 1
            },
            "explosive_arrows_lvl_1": {
                "id": 462,
                "kills_a_number_of_ranged_attackers_in_the_courtyard": 100,
                "strength_in_courtyard_when_defending": 1
            },
            "high_queens_banner": {
                "id": 474,
                "glory_points": 11
            },
            "high_kings_banner": {
                "id": 475,
                "glory_points": 12
            },
            "supreme_monarchs_banner": {
                "id": 476,
                "glory_points": 13
            },
            "giant_nomad_tablet_chest": {
                "id": 490,
                "khan_tablet_bonus": 11
            },
            "enormous_nomad_tablet_chest": {
                "id": 491,
                "khan_tablet_bonus": 12
            },
            "colossal_nomad_tablet_chest": {
                "id": 492,
                "khan_tablet_bonus": 13
            },
            "giant_samurai_token_chest": {
                "id": 510,
                "samurai_tokens_bonus": 9
            },
            "enormous_samurai_token_chest": {
                "id": 511,
                "samurai_tokens_bonus": 10
            },
            "colossal_samurai_token_chest": {
                "id": 512,
                "samurai_tokens_bonus": 11
            },
            "ram_of_the_shogun_557": {
                "id": 557,
                "gate_reduction": 25,
                "samurai_tokens_bonus": 6
            },
            "shield_of_the_shogun_558": {
                "id": 558,
                "range_reduction": 20,
                "samurai_tokens_bonus": 6
            },
            "ladder_of_the_shogun_559": {
                "id": 559,
                "wall_reduction": 25,
                "samurai_tokens_bonus": 6
            },
            "elite_nomad_ram_560": {
                "id": 560,
                "gate_reduction": 25,
                "khan_tablet_bonus": 7
            },
            "elite_nomad_rope_ladder_561": {
                "id": 561,
                "wall_reduction": 25,
                "khan_tablet_bonus": 7
            },
            "elite_nomad_shield_562": {
                "id": 562,
                "range_reduction": 20,
                "khan_tablet_bonus": 7
            },
            "rage_banner_563": {
                "id": 563,
                "rage_bonus": 2
            },
            "wolverine_ram_564": {
                "id": 564,
                "gate_reduction": 20,
                "glory_points": 9
            },
            "elite_tonnelon_565": {
                "id": 565,
                "wall_reduction": 20,
                "glory_points": 9
            },
            "invader_mask_566": {
                "id": 566,
                "range_reduction": 20,
                "glory_points": 9
            },
            "raid_ladder_60": {
                "id": 568,
                "wall_reduction": 60
            },
            "exalted_raid_ladder_250": {
                "id": 569,
                "wall_reduction": 250
            },
            "raid_ram_60": {
                "id": 570,
                "gate_reduction": 60
            },
            "exalted_raid_ram_250": {
                "id": 571,
                "gate_reduction": 250
            },
            "raid_dominator_ladder": {
                "id": 572,
                "wall_reduction": 125,
                "rift_points_bonus": 1
            },
            "master_rift_dominator_ladder": {
                "id": 573,
                "wall_reduction": 340,
                "rift_points_bonus": 3
            },
            "raid_dominator_ram": {
                "id": 574,
                "gate_reduction": 125,
                "rift_points_bonus": 1
            },
            "master_rift_dominator_ram": {
                "id": 575,
                "gate_reduction": 340,
                "rift_points_bonus": 3
            },
            "raid_dominator_banner": {
                "id": 576,
                "rift_points_bonus": 100,
                "tool_limit_per_wave": 1
            },
            "veteran_dominator_banner": {
                "id": 577,
                "rift_points_bonus": 250,
                "tool_limit_per_wave": 1
            },
            "master_dominator_banner": {
                "id": 578,
                "rift_points_bonus": 500,
                "tool_limit_per_wave": 1
            },
            "graveguard_potion": {
                "id": 579,
                "zombie_infection_rate_reduction": 5,
                "tool_limit_per_wave": 1
            },
            "great_graveguard_potion": {
                "id": 580,
                "zombie_infection_rate_reduction": 15,
                "tool_limit_per_wave": 1
            },
            "premium_graveguard_potion": {
                "id": 581,
                "zombie_infection_rate_reduction": 30,
                "tool_limit_per_wave": 1
            },
            "raid_ladder_80": {
                "id": 583,
                "wall_reduction": 80
            },
            "raid_ladder_125": {
                "id": 584,
                "wall_reduction": 125
            },
            "raid_ladder_200": {
                "id": 585,
                "wall_reduction": 200
            },
            "exalted_raid_ladder_340": {
                "id": 586,
                "wall_reduction": 340
            },
            "exalted_raid_ladder_500": {
                "id": 587,
                "wall_reduction": 500
            },
            "raid_ram_80": {
                "id": 588,
                "gate_reduction": 80
            },
            "raid_ram_125": {
                "id": 589,
                "gate_reduction": 125
            },
            "raid_ram_200": {
                "id": 590,
                "gate_reduction": 200
            },
            "exalted_raid_ram_340": {
                "id": 591,
                "gate_reduction": 340
            },
            "exalted_raid_ram_500": {
                "id": 592,
                "gate_reduction": 500
            },
            "battering_ram": {
                "id": 611,
                "gate_reduction": 10
            },
            "scaling_ladder": {
                "id": 614,
                "wall_reduction": 10
            },
            "wood_bundle": {
                "id": 617,
                "moat_reduction": 5
            },
            "mantlet": {
                "id": 620,
                "range_reduction": 5
            },
            "portcullis": {
                "id": 622,
                "gate_protection": 75
            },
            "bamboo_ladder": {
                "id": 623,
                "wall_reduction": 25
            },
            "arrow_slit": {
                "id": 624,
                "ranged_defence": 70
            },
            "fire_moat": {
                "id": 625,
                "moat_protection": 110
            },
            "castle_gate_reinforcement": {
                "id": 626,
                "gate_protection": 35
            },
            "murder_hole": {
                "id": 627,
                "wall_protection": 50
            },
            "flaming_arrows_629": {
                "id": 629,
                "ranged_defence": 25
            },
            "dummy_melee_soldiers": {
                "id": 632
            },
            "dummy_ranged_soldiers": {
                "id": 633
            },
            "sharpened_stakes_634": {
                "id": 634,
                "moat_protection": 35
            },
            "cast_iron_mantlet": {
                "id": 635,
                "range_reduction": 10
            },
            "hurling_rocks_637": {
                "id": 637,
                "wall_protection": 25
            },
            "moon_storm_banner": {
                "id": 638,
                "melee_attack": 30
            },
            "dummy_archers": {
                "id": 639
            },
            "siege_tower": {
                "id": 640,
                "wall_reduction": 15
            },
            "iron_ram": {
                "id": 641,
                "gate_reduction": 15
            },
            "assault_bridge": {
                "id": 642,
                "moat_reduction": 10
            },
            "grimacing_shield": {
                "id": 643,
                "range_reduction": 20
            },
            "insulating_mat_644": {
                "id": 644,
                "gate_protection": 60
            },
            "bulwark": {
                "id": 645,
                "ranged_defence": 50
            },
            "swamp_snapper": {
                "id": 646,
                "moat_protection": 80
            },
            "tar_pitch_kettle": {
                "id": 647,
                "wall_protection": 40
            },
            "heavy_ram": {
                "id": 648,
                "gate_reduction": 20
            },
            "breaching_tower": {
                "id": 649,
                "wall_reduction": 20
            },
            "boulders": {
                "id": 650,
                "moat_reduction": 15
            },
            "shield_wall": {
                "id": 651,
                "range_reduction": 15
            },
            "loot_sack": {
                "id": 653,
                "loot_capacity": 15
            },
            "loot_cart": {
                "id": 654,
                "loot_capacity": 35
            },
            "banner": {
                "id": 660,
                "glory_points": 1
            },
            "war_banner": {
                "id": 661,
                "glory_points": 2
            },
            "large_ballista": {
                "id": 669,
                "ranged_attack": 30
            },
            "lime_powder_bomb": {
                "id": 730,
                "melee_defence": 33
            },
            "bodkin_arrowheads": {
                "id": 731,
                "ranged_defence": 50
            },
            "turtle_ram": {
                "id": 732,
                "gate_reduction": 15
            },
            "grappling_hooks": {
                "id": 733,
                "wall_reduction": 15
            },
            "horsetail_standard": {
                "id": 734,
                "glory_points": 5
            },
            "dummy_peasants": {
                "id": 735,
                "range_reduction": 10
            },
            "transport_yak": {
                "id": 736,
                "loot_capacity": 40
            },
            "war_horn": {
                "id": 737,
                "melee_attack": 1
            },
            "fire_cart": {
                "id": 738,
                "gate_reduction": 25,
                "khan_tablet_bonus": 3
            },
            "rolling_shield": {
                "id": 739,
                "range_reduction": 20,
                "khan_tablet_bonus": 3
            },
            "tree_trunk_ladder": {
                "id": 740,
                "wall_reduction": 25,
                "khan_tablet_bonus": 3
            },
            "torches": {
                "id": 741,
                "melee_attack": 30
            },
            "poison_arrows": {
                "id": 742,
                "ranged_attack": 30
            },
            "veteran_banner": {
                "id": 745,
                "tool_limit_per_wave": 1,
                "experience_points_bonus": 50
            },
            "thorn_ladder": {
                "id": 755,
                "wall_reduction": 25
            },
            "wedge_shield": {
                "id": 756,
                "range_reduction": 20
            },
            "banner_of_the_demon_slayers": {
                "id": 757,
                "melee_attack": 30
            },
            "ballista": {
                "id": 758,
                "ranged_attack": 30
            },
            "boarding_ladder": {
                "id": 761,
                "wall_reduction": 25,
                "pearl_bonus": 3
            },
            "shark_shield": {
                "id": 762,
                "range_reduction": 20,
                "pearl_bonus": 3
            },
            "kraken_banner": {
                "id": 763,
                "melee_attack": 30,
                "pearl_bonus": 3
            },
            "nautical_ballista": {
                "id": 764,
                "ranged_attack": 30,
                "pearl_bonus": 3
            },
            "invader_ram": {
                "id": 770,
                "gate_reduction": 25
            },
            "invader_ladder": {
                "id": 771,
                "wall_reduction": 25
            },
            "invader_bridge": {
                "id": 772,
                "moat_reduction": 20
            },
            "invader_mask_773": {
                "id": 773,
                "range_reduction": 20
            },
            "banner_of_the_foreign_invaders": {
                "id": 774,
                "glory_points": 4
            },
            "the_claw": {
                "id": 775,
                "gate_reduction": 25,
                "gallantry_points_bonus": 3
            },
            "paw_ladder": {
                "id": 776,
                "wall_reduction": 25,
                "gallantry_points_bonus": 3
            },
            "fanged_bridge": {
                "id": 777,
                "moat_reduction": 20,
                "gallantry_points_bonus": 3
            },
            "berimond_shield": {
                "id": 778,
                "range_reduction": 20,
                "gallantry_points_bonus": 3
            },
            "the_glory_of_berimond": {
                "id": 779,
                "glory_points": 4
            },
            "the_pride_of_berimond": {
                "id": 780,
                "tool_limit_per_wave": 1,
                "gallantry_points_bonus": 25
            },
            "bronze_magmatic_chrono_vial_left": {
                "id": 789,
                "seconds_to_left_wall_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "bronze_magmatic_chrono_vial_gate": {
                "id": 790,
                "seconds_to_gate_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "bronze_magmatic_chrono_vial_right": {
                "id": 791,
                "seconds_to_right_wall_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "bronze_magmatic_chrono_vial_all_sides": {
                "id": 792,
                "seconds_to_all_flanks_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "silver_magmatic_chrono_vial_left": {
                "id": 793,
                "seconds_to_left_wall_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "silver_magmatic_chrono_vial_gate": {
                "id": 794,
                "seconds_to_gate_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "silver_magmatic_chrono_vial_right": {
                "id": 795,
                "seconds_to_right_wall_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "silver_magmatic_chrono_vial_all_sides": {
                "id": 796,
                "seconds_to_all_flanks_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "gold_magmatic_chrono_vial_left": {
                "id": 797,
                "seconds_to_left_wall_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "gold_magmatic_chrono_vial_gate": {
                "id": 798,
                "seconds_to_gate_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "gold_magmatic_chrono_vial_right": {
                "id": 799,
                "seconds_to_right_wall_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "gold_magmatic_chrono_vial_all_sides": {
                "id": 800,
                "seconds_to_all_flanks_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "bronze_magmatic_grenade_flask": {
                "id": 801,
                "instantly_kills_dormant_rift_egg_units_from_the_enemy_reserve": 5000,
                "tool_limit_per_wave": 1
            },
            "silver_magmatic_grenade_flask": {
                "id": 802,
                "instantly_kills_dormant_rift_egg_units_from_the_enemy_reserve": 50000,
                "tool_limit_per_wave": 1
            },
            "gold_magmatic_grenade_flask": {
                "id": 803,
                "instantly_kills_dormant_rift_egg_units_from_the_enemy_reserve": 250000,
                "tool_limit_per_wave": 1
            },
            "bronze_magmatic_flintlock_gun": {
                "id": 804,
                "instantly_kills_dormant_rift_wyrmling_units_from_the_enemy_reserve": 1000,
                "tool_limit_per_wave": 1
            },
            "silver_magmatic_flintlock_gun": {
                "id": 805,
                "instantly_kills_dormant_rift_wyrmling_units_from_the_enemy_reserve": 10000,
                "tool_limit_per_wave": 1
            },
            "gold_magmatic_flintlock_gun": {
                "id": 806,
                "instantly_kills_dormant_rift_wyrmling_units_from_the_enemy_reserve": 50000,
                "tool_limit_per_wave": 1
            },
            "bronze_obsidian_weighted_net": {
                "id": 807,
                "melee_defence_strength_reduction": 50
            },
            "silver_obsidian_weighted_net": {
                "id": 808,
                "melee_defence_strength_reduction": 250
            },
            "gold_obsidian_weighted_net": {
                "id": 809,
                "melee_defence_strength_reduction": 500
            },
            "bronze_obsidian_siege_shield": {
                "id": 810,
                "melee_defence_strength_reduction": 50
            },
            "silver_obsidian_siege_shield": {
                "id": 811,
                "melee_defence_strength_reduction": 250
            },
            "gold_obsidian_siege_shield": {
                "id": 812,
                "melee_defence_strength_reduction": 500
            },
            "throwing_nails_lvl_1": {
                "id": 840,
                "melee_defence": 18
            },
            "karimata_arrowheads_lvl_1": {
                "id": 850,
                "ranged_defence": 29
            },
            "arrow_rain_ballista_lvl_1": {
                "id": 880,
                "ranged_attack": 18
            },
            "smoke_bomb_lvl_1": {
                "id": 890,
                "melee_attack": 18
            },
            "lime_powder_bomb_lvl_1": {
                "id": 920,
                "melee_defence": 18
            },
            "bodkin_arrowheads_lvl_1": {
                "id": 930,
                "ranged_defence": 29
            },
            "bronze_sporeguard_chrono_vial_left": {
                "id": 964,
                "seconds_to_left_wall_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "bronze_sporeguard_chrono_vial_gate": {
                "id": 965,
                "seconds_to_gate_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "bronze_sporeguard_chrono_vial_right": {
                "id": 966,
                "seconds_to_right_wall_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "bronze_sporeguard_chrono_vial_all_sides": {
                "id": 967,
                "seconds_to_all_flanks_regeneration_cooldown_after_breaching": 30,
                "tool_limit_per_wave": 1
            },
            "silver_sporeguard_chrono_vial_left": {
                "id": 968,
                "seconds_to_left_wall_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "silver_sporeguard_chrono_vial_gate": {
                "id": 969,
                "seconds_to_gate_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "silver_sporeguard_chrono_vial_right": {
                "id": 970,
                "seconds_to_right_wall_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "silver_sporeguard_chrono_vial_all_sides": {
                "id": 971,
                "seconds_to_all_flanks_regeneration_cooldown_after_breaching": 90,
                "tool_limit_per_wave": 1
            },
            "gold_sporeguard_chrono_vial_left": {
                "id": 972,
                "seconds_to_left_wall_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "gold_sporeguard_chrono_vial_gate": {
                "id": 973,
                "seconds_to_gate_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "gold_sporeguard_chrono_vial_right": {
                "id": 974,
                "seconds_to_right_wall_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "gold_sporeguard_chrono_vial_all_sides": {
                "id": 975,
                "seconds_to_all_flanks_regeneration_cooldown_after_breaching": 270,
                "tool_limit_per_wave": 1
            },
            "bronze_sporebane_grenade_flask": {
                "id": 976,
                "instantly_kills_dormant_spore_maw_units_from_the_enemy_reserve": 100000,
                "tool_limit_per_wave": 1
            },
            "silver_sporebane_grenade_flask": {
                "id": 977,
                "instantly_kills_dormant_spore_maw_units_from_the_enemy_reserve": 250000,
                "tool_limit_per_wave": 1
            },
            "gold_sporebane_grenade_flask": {
                "id": 978,
                "instantly_kills_dormant_spore_maw_units_from_the_enemy_reserve": 600000,
                "tool_limit_per_wave": 1
            },
            "bronze_sporebane_flintlock_gun": {
                "id": 979,
                "instantly_kills_dormant_spore_cyst_units_from_the_enemy_reserve": 100000,
                "tool_limit_per_wave": 1
            },
            "silver_sporebane_flintlock_gun": {
                "id": 980,
                "instantly_kills_dormant_spore_cyst_units_from_the_enemy_reserve": 250000,
                "tool_limit_per_wave": 1
            },
            "gold_sporebane_flintlock_gun": {
                "id": 981,
                "instantly_kills_dormant_spore_cyst_units_from_the_enemy_reserve": 600000,
                "tool_limit_per_wave": 1
            },
        }
        return mapping[self.value]

    @property
    def id(self):
        """Tool ID"""
        return self.data["id"]
